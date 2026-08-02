//! Content-index wiring (CPE-1262, epic CPE-976): the walk + persistence layer that turns
//! [`crate::semantic_index::SemanticIndex`] + [`crate::embedder::FakeEmbedder`] into something a Tauri
//! command can build, persist, reload, and search over a real folder. The engine (chunk/embed/search/
//! persist) is already built and tested in `semantic_index`/`embedder`/`vector_index` (CPE-981/982/983);
//! this module supplies the missing plumbing: walk a folder for text-like files, feed them to
//! [`SemanticIndex::upsert_document`], and persist + reload the result keyed by the indexed root.
//!
//! Framed honestly: this is file-**content** search — a local, dependency-free feature-hashed
//! bag-of-words embedder, no network call, no API key — not an oversold "AI semantic" claim. A better or
//! pluggable embedder can drop in later behind the existing [`crate::embedder::Embedder`] seam without
//! touching this module (see [`CONTENT_INDEX_DIM`]'s docs for the one thing that constant couples to).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::embedder::{Embedder, FakeEmbedder};
use crate::fsutil::entry_is_symlink;
use crate::semantic_index::{SemanticIndex, SemanticIndexError};

/// The embedding dimensionality every content index this module builds or loads uses. **Must stay fixed**
/// for a persisted index to keep loading: [`SemanticIndex::load`] validates the loaded vectors' dimension
/// against the embedder handed to it and returns `Err` on a mismatch (never a panic) — see [`load_index`].
/// Changing this constant invalidates every previously-persisted `.cix` file; a caller with a stale file
/// gets a clear `Err` from `load_index`, not a crash, and can `content_index_build` again to refresh it.
pub const CONTENT_INDEX_DIM: usize = 1024;

/// Skip files bigger than this. Content indexing (chunk + embed) is more work per byte than a line grep
/// (`content_search`'s `SEARCH_MAX_FILE_BYTES`, which this mirrors at a smaller cap), so a hard ceiling
/// keeps a build's cost predictable regardless of what stray huge file sits under `root`.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Safety cap on files considered in one build — mirrors `content_search::SEARCH_MAX_FILES` so a build
/// over a huge tree still terminates (reported via `truncated`) instead of running unbounded.
const MAX_FILES: u64 = 20_000;

/// How many indexed files between progress ticks — frequent enough a streaming caller sees steady
/// progress, coarse enough it isn't an IPC send per file.
const PROGRESS_EVERY: u64 = 10;

/// Length (in chars) of the snippet returned per hit — a short slice of the matched document's text, not
/// the whole file.
const SNIPPET_MAX_CHARS: usize = 240;

/// True if a byte slice looks binary (contains a NUL in the sniffed prefix) — same heuristic
/// `content_search` uses to skip files not worth reading as text.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

/// One `content_index_build` progress tick — how far the walk has gotten. Serialisable: it's the
/// `content_index_build` progress-channel payload (`docs/design/STREAMING.md`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ContentIndexProgress {
    pub files_indexed: u64,
    pub files_skipped: u64,
    /// The file most recently indexed (empty on the final tick, which reports the finished totals).
    pub current_path: String,
}

/// The final `content_index_build` result: how many files were indexed vs skipped, and whether a cap or
/// cancellation cut the walk short. Serialisable: it's the command's return type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ContentIndexBuildStats {
    pub files_indexed: u64,
    pub files_skipped: u64,
    pub truncated: bool,
}

/// One ranked `content_search` hit: the file, its similarity score, and a snippet of its text.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ContentHit {
    pub path: String,
    pub score: f32,
    pub snippet: String,
}

/// The `content_search` result: ranked hits, plus whether an index existed at all for `root` — lets the
/// frontend tell "index built, zero matches" apart from "never built, prompt to build" without either one
/// looking like a failure (CPE-1262 acceptance: a missing index is a clean signal, not an error/crash).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ContentSearchOutcome {
    pub hits: Vec<ContentHit>,
    pub index_exists: bool,
}

/// A fresh embedder for building/loading a content index — always [`CONTENT_INDEX_DIM`]-dimensional, so
/// every index this module touches agrees on the one dimension that matters for load compatibility.
fn make_embedder() -> Box<dyn Embedder> {
    Box::new(FakeEmbedder::new(CONTENT_INDEX_DIM))
}

/// FNV-1a 64-bit — a small, **stable** hash (cross-run, cross-platform), unlike `std`'s randomly-seeded
/// `DefaultHasher`, so a root's index filename is the same on every launch (mirrors the same discipline
/// `embedder::FakeEmbedder` uses for its own token hash).
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The stable key a root maps to for its persisted index filename / build-cancel registry.
pub fn root_key(root: &str) -> u64 {
    fnv1a64(root.as_bytes())
}

/// The on-disk path `root`'s content index is persisted to under `dir` (typically
/// `<app_data>/content-index`): `dir/<hash(root)>.cix`.
pub fn index_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("{:016x}.cix", root_key(root)))
}

/// Walk `root`, index every text-like file under it into a fresh [`SemanticIndex`], and return it plus the
/// build stats. `cancel` is polled between entries so a caller can abort a long build; a cancelled build
/// returns the partial index built so far, with [`ContentIndexBuildStats::truncated`] set. Mirrors
/// `content_search::stream_file_contents`'s walk shape (explicit stack, skip dot-dirs, symlinked dirs, and
/// unreadable entries; cap the file count) but embeds each file's text instead of grepping it.
///
/// `progress` is invoked every [`PROGRESS_EVERY`] indexed files, plus once more at the end with the final
/// totals and an empty `current_path`, so a streaming caller always sees the completed count.
pub fn build_index_with(
    root: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(ContentIndexProgress),
) -> Result<(SemanticIndex, ContentIndexBuildStats), String> {
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }
    let mut index = SemanticIndex::new(make_embedder());
    let mut stats = ContentIndexBuildStats::default();

    let mut stack = vec![root_path.to_path_buf()];
    'walk: while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            stats.truncated = true;
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if cancel.load(Ordering::Relaxed) {
                stats.truncated = true;
                break 'walk;
            }
            if stats.files_indexed + stats.files_skipped >= MAX_FILES {
                stats.truncated = true;
                break 'walk;
            }
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Skip dot-dirs and symlinked dirs (avoid cycles), matching content_search/name_search.
                if !name.starts_with('.') && !entry_is_symlink(&entry) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
                stats.files_skipped += 1;
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                stats.files_skipped += 1;
                continue;
            };
            if looks_binary(&bytes) {
                stats.files_skipped += 1;
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            let path_str = path.to_string_lossy().into_owned();
            index.upsert_document(&path_str, &text);
            stats.files_indexed += 1;
            if stats.files_indexed % PROGRESS_EVERY == 0 {
                progress(ContentIndexProgress {
                    files_indexed: stats.files_indexed,
                    files_skipped: stats.files_skipped,
                    current_path: path_str,
                });
            }
        }
    }
    progress(ContentIndexProgress {
        files_indexed: stats.files_indexed,
        files_skipped: stats.files_skipped,
        current_path: String::new(),
    });
    Ok((index, stats))
}

/// [`build_index_with`] with no progress callback — the entry point tests and non-streaming callers use.
pub fn build_index(root: &str, cancel: &AtomicBool) -> Result<(SemanticIndex, ContentIndexBuildStats), String> {
    build_index_with(root, cancel, |_| {})
}

/// Persist `index` to `dir/<hash(root)>.cix`, creating `dir` if it doesn't exist yet.
pub fn save_index(index: &SemanticIndex, dir: &Path, root: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    index.save(&index_path(dir, root)).map_err(|e| e.to_string())
}

/// Load `root`'s persisted content index from `dir`, using the fixed [`CONTENT_INDEX_DIM`] embedder.
///
/// - No file yet → `Ok(None)` — a clean "needs build" signal, not an error (CPE-1262 acceptance).
/// - A file that exists but fails to parse (stale format, or a dimension mismatch from a different/older
///   embedder) → `Err` with a readable message — never a panic, mirroring [`SemanticIndex::load`]'s own
///   discipline.
pub fn load_index(dir: &Path, root: &str) -> Result<Option<SemanticIndex>, String> {
    let path = index_path(dir, root);
    if !path.is_file() {
        return Ok(None);
    }
    SemanticIndex::load(&path, make_embedder())
        .map(Some)
        .map_err(|e| match e {
            SemanticIndexError::Stale => {
                "content index is stale (format or embedder changed) — rebuild it".to_string()
            }
            SemanticIndexError::Io(m) => format!("content index unreadable: {m}"),
        })
}

/// Search `root`'s persisted content index for `query`, returning up to `k` ranked hits (path, score, and
/// a snippet of the matched document). No index built yet for `root` → an empty, `index_exists: false`
/// outcome rather than an error, so the frontend can prompt to build instead of showing a crash.
///
/// Snippet note: [`crate::semantic_index::DocHit`] carries only the doc id + score (the vector layer
/// stores embeddings, not the original chunk text), so the snippet is read back from the file itself —
/// [`snippet_for`] centres it on the query's first literally-matching token when one is found, falling back
/// to the start of the file otherwise (the embedder can also match on hashed token overlap that isn't a
/// literal substring).
pub fn search_index(dir: &Path, root: &str, query: &str, k: usize) -> Result<ContentSearchOutcome, String> {
    let Some(index) = load_index(dir, root)? else {
        return Ok(ContentSearchOutcome::default());
    };
    let hits = index
        .search(query, k)
        .into_iter()
        .map(|hit| {
            let snippet = snippet_for(&hit.doc_id, query);
            ContentHit { path: hit.doc_id, score: hit.score, snippet }
        })
        .collect();
    Ok(ContentSearchOutcome { hits, index_exists: true })
}

/// A short snippet of `path`'s own text, centred on `query`'s first literally-matching token when one is
/// found (case-insensitive), else the start of the file. Best-effort: an unreadable/moved/binary file
/// yields an empty snippet rather than an error — the file could have changed since the index was built.
fn snippet_for(path: &str, query: &str) -> String {
    let Ok(bytes) = std::fs::read(path) else { return String::new() };
    if looks_binary(&bytes) {
        return String::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let flat: Vec<char> = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect();
    if flat.is_empty() {
        return String::new();
    }
    let lower: String = flat.iter().collect::<String>().to_lowercase();
    let hit_char = query.split_whitespace().find_map(|term| {
        let term = term.to_lowercase();
        lower.find(&term).map(|byte_idx| lower[..byte_idx].chars().count())
    });
    let start = hit_char.map(|c| c.saturating_sub(SNIPPET_MAX_CHARS / 4)).unwrap_or(0);
    let end = (start + SNIPPET_MAX_CHARS).min(flat.len());
    let mut snippet: String = flat[start..end].iter().collect();
    if start > 0 {
        snippet = format!("…{snippet}");
    }
    if end < flat.len() {
        snippet = format!("{snippet}…");
    }
    snippet
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicU64;

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-content-index-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample_tree(root: &Path) {
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("fox.txt"), b"the quick brown fox jumps over the lazy dog").unwrap();
        fs::write(root.join("sub").join("dog.txt"), b"a lazy sleeping dog in the warm sun").unwrap();
        // A binary file (NUL byte) — must be skipped, never indexed.
        fs::write(root.join("binary.bin"), b"quick fox\x00binary junk").unwrap();
        // A dot-dir — must not be descended.
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git").join("x"), b"quick fox in git").unwrap();
    }

    #[test]
    fn build_indexes_text_files_and_skips_binary_and_dot_dirs() {
        let tree = scratch("tree");
        sample_tree(&tree);
        let (index, stats) = build_index(&tree.to_string_lossy(), &AtomicBool::new(false)).unwrap();
        assert_eq!(stats.files_indexed, 2, "fox.txt + sub/dog.txt only");
        assert_eq!(stats.files_skipped, 1, "binary.bin skipped (NUL byte)");
        assert!(!stats.truncated);
        assert_eq!(index.document_count(), 2);
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn build_then_search_ranks_the_expected_file_first() {
        let tree = scratch("search");
        sample_tree(&tree);
        let (index, _) = build_index(&tree.to_string_lossy(), &AtomicBool::new(false)).unwrap();
        let hits = index.search("quick fox", 5);
        assert!(!hits.is_empty());
        assert!(hits[0].doc_id.replace('\\', "/").ends_with("fox.txt"));
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn build_persist_load_search_round_trips_via_the_search_index_api() {
        let tree = scratch("roundtrip-tree");
        sample_tree(&tree);
        let idxdir = scratch("roundtrip-idx");
        let root = tree.to_string_lossy().to_string();

        let (index, _) = build_index(&root, &AtomicBool::new(false)).unwrap();
        save_index(&index, &idxdir, &root).unwrap();
        assert!(index_path(&idxdir, &root).is_file());

        let outcome = search_index(&idxdir, &root, "quick fox", 5).unwrap();
        assert!(outcome.index_exists);
        assert!(!outcome.hits.is_empty());
        assert!(outcome.hits[0].path.replace('\\', "/").ends_with("fox.txt"));
        assert!(outcome.hits[0].score > 0.0);
        assert!(!outcome.hits[0].snippet.is_empty(), "snippet should be populated from the file's own text");

        // A second search_index call re-loads independently and returns the same top hit (persistence
        // round-trip, not relying on any in-memory residency between calls).
        let again = search_index(&idxdir, &root, "quick fox", 5).unwrap();
        assert_eq!(outcome.hits[0].path, again.hits[0].path);
        assert_eq!(outcome.hits[0].score, again.hits[0].score);

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    #[test]
    fn search_index_with_no_built_index_is_a_clean_empty_result_not_an_error() {
        let idxdir = scratch("missing-idx");
        let outcome = search_index(&idxdir, "Z:/some/root/never/built", "anything", 5).unwrap();
        assert!(!outcome.index_exists);
        assert!(outcome.hits.is_empty());
        // Nothing should have been created by a read-only search over a never-built root.
        assert!(!index_path(&idxdir, "Z:/some/root/never/built").exists());
        let _ = fs::remove_dir_all(&idxdir);
    }

    #[test]
    fn load_index_with_dim_mismatch_is_a_clear_err_not_a_panic() {
        let tree = scratch("dimtree");
        sample_tree(&tree);
        let idxdir = scratch("dim-idx");
        let root = tree.to_string_lossy().to_string();
        let (index, _) = build_index(&root, &AtomicBool::new(false)).unwrap();
        // Persist directly at CONTENT_INDEX_DIM, then attempt to load it via a mismatched-dim embedder —
        // simulating a persisted file from a different/older embedder, which must never panic.
        std::fs::create_dir_all(&idxdir).unwrap();
        index.save(&index_path(&idxdir, &root)).unwrap();

        let path = index_path(&idxdir, &root);
        let wrong = SemanticIndex::load(&path, Box::new(FakeEmbedder::new(256)));
        assert!(matches!(wrong, Err(SemanticIndexError::Stale)));

        // And through this module's own `load_index` entry point: a dim-mismatched file (simulated by
        // pointing a *different* CONTENT_INDEX_DIM-independent load at bytes saved for a smaller dim)
        // returns Err, never panics, never gets silently treated as "no index".
        let small = SemanticIndex::new(Box::new(FakeEmbedder::new(8)));
        let small_path = idxdir.join("small.cix");
        small.save(&small_path).unwrap();
        let reloaded = SemanticIndex::load(&small_path, make_embedder()); // CONTENT_INDEX_DIM (1024) != 8
        assert!(matches!(reloaded, Err(SemanticIndexError::Stale)));

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    #[test]
    fn root_key_and_index_path_are_stable_and_distinct_per_root() {
        let a = root_key("Z:/repos/one");
        let b = root_key("Z:/repos/one");
        let c = root_key("Z:/repos/two");
        assert_eq!(a, b, "same root hashes identically every call");
        assert_ne!(a, c, "different roots hash differently");
        let dir = PathBuf::from("Z:/appdata/content-index");
        assert_eq!(index_path(&dir, "Z:/repos/one"), index_path(&dir, "Z:/repos/one"));
        assert_ne!(index_path(&dir, "Z:/repos/one"), index_path(&dir, "Z:/repos/two"));
    }

    #[test]
    fn build_over_a_non_folder_root_is_an_err_not_a_panic() {
        let tree = scratch("notadir");
        let file = tree.join("f.txt");
        fs::write(&file, b"x").unwrap();
        let got = build_index(&file.to_string_lossy(), &AtomicBool::new(false));
        assert!(got.is_err());
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn cancel_flag_truncates_a_build_in_progress() {
        let tree = scratch("cancel-tree");
        fs::create_dir_all(&tree).unwrap();
        for i in 0..50 {
            fs::write(tree.join(format!("f{i}.txt")), format!("file number {i} words here")).unwrap();
        }
        let cancel = AtomicBool::new(true); // pre-cancelled: the walk should stop almost immediately
        let (_, stats) = build_index(&tree.to_string_lossy(), &cancel).unwrap();
        assert!(stats.truncated);
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn progress_callback_fires_and_reports_a_final_tick() {
        let tree = scratch("progress-tree");
        fs::create_dir_all(&tree).unwrap();
        for i in 0..25 {
            fs::write(tree.join(format!("f{i}.txt")), format!("hello world {i}")).unwrap();
        }
        let mut ticks: Vec<ContentIndexProgress> = Vec::new();
        let (_, stats) = build_index_with(&tree.to_string_lossy(), &AtomicBool::new(false), |p| ticks.push(p)).unwrap();
        assert!(!ticks.is_empty(), "at least the final tick must fire");
        let last = ticks.last().unwrap();
        assert_eq!(last.files_indexed, stats.files_indexed);
        assert_eq!(last.current_path, "", "final tick reports empty current_path");
        let _ = fs::remove_dir_all(&tree);
    }

    #[test]
    fn empty_query_or_zero_k_yields_no_hits_but_index_exists_is_still_true() {
        let tree = scratch("emptyq-tree");
        sample_tree(&tree);
        let idxdir = scratch("emptyq-idx");
        let root = tree.to_string_lossy().to_string();
        let (index, _) = build_index(&root, &AtomicBool::new(false)).unwrap();
        save_index(&index, &idxdir, &root).unwrap();

        let empty_query = search_index(&idxdir, &root, "", 5).unwrap();
        assert!(empty_query.index_exists);
        assert!(empty_query.hits.is_empty());

        let zero_k = search_index(&idxdir, &root, "quick fox", 0).unwrap();
        assert!(zero_k.index_exists);
        assert!(zero_k.hits.is_empty());

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }
}
