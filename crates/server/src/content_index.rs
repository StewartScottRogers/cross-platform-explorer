//! Content-index wiring (CPE-1262, epic CPE-976): the walk + persistence layer that turns
//! [`crate::semantic_index::SemanticIndex`] + [`crate::embedder::FakeEmbedder`] into something a Tauri
//! command can build, persist, reload, and search over a real folder. The engine (chunk/embed/search/
//! persist) is already built and tested in `semantic_index`/`embedder`/`vector_index` (CPE-981/982/983);
//! this module supplies the missing plumbing: walk a folder for text-like (and, since CPE-1274,
//! text-*extractable* — PDF/DOCX/XLSX/PPTX) files, feed them to [`SemanticIndex::upsert_document`], and
//! persist + reload the result keyed by the indexed root. The per-file bytes→text step is
//! [`crate::content_text::content_text_of`] — see that module for the extraction dispatch itself; this
//! module only owns the walk, the persistence, and the search/snippet plumbing around it.
//!
//! Framed honestly: this is file-**content** search — a local, dependency-free feature-hashed
//! bag-of-words embedder, no network call, no API key — not an oversold "AI semantic" claim. A better or
//! pluggable embedder can drop in later behind the existing [`crate::embedder::Embedder`] seam without
//! touching this module (see [`CONTENT_INDEX_DIM`]'s docs for the one thing that constant couples to).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

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

/// A fresh **default** embedder for building/loading a content index — the local, dependency-free
/// [`FakeEmbedder`] at [`CONTENT_INDEX_DIM`]. This is the off/unconfigured path (CPE-1273): when no real
/// embedder is enabled, everything is byte-identical to pre-CPE-1273 behavior (same embedder, same
/// untagged index path). A real embedder (CPE-1273) is selected via [`resolve_configured_embedder`] and
/// used through the `*_with_embedder`/`*_tagged`/`*_configured` entry points below.
fn make_embedder() -> Box<dyn Embedder> {
    Box::new(FakeEmbedder::new(CONTENT_INDEX_DIM))
}

/// Persisted selection of the embedder content search uses (CPE-1273, epic CPE-976). Off by default →
/// the local [`FakeEmbedder`] (current behavior). When `enabled` with a `base_url` + `model`, an
/// OpenAI-compatible [`crate::http_embedder::HttpEmbedder`] is used instead. The API **key is NOT here**
/// — it lives in the OS keychain and is fetched separately at the command boundary and passed to
/// [`resolve_configured_embedder`], so a key never persists in plaintext settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ContentEmbedderConfig {
    /// When false (the default), content search uses the local [`FakeEmbedder`] and the legacy untagged
    /// index — no network, no key.
    pub enabled: bool,
    /// The embeddings server base URL, with or without `/v1` (e.g. `http://localhost:1234/v1` for LM
    /// Studio, or `https://api.openai.com/v1`). See [`crate::http_embedder::embeddings_url`].
    pub base_url: String,
    /// The embedding model name the server expects (e.g. `text-embedding-3-small`, or a local model id).
    pub model: String,
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
/// `<app_data>/content-index`): `dir/<hash(root)>.cix`. This is the **default/local** (FakeEmbedder)
/// path — untagged, so the off/unconfigured path stays byte-identical to pre-CPE-1273.
pub fn index_path(dir: &Path, root: &str) -> PathBuf {
    dir.join(format!("{:016x}.cix", root_key(root)))
}

/// The on-disk path for `root`'s index built with a **non-default embedder**, keyed by that embedder's
/// identity `tag` (CPE-1273): `dir/<hash(root)>-<tag>.cix`. Keying by embedder identity means switching
/// embedders (or model, or dim) points at a *different* file, so a switch cleanly reads as "needs build"
/// (a rebuild) rather than serving vectors a different model produced. See [`http_embedder_tag`].
pub fn index_path_for(dir: &Path, root: &str, tag: &str) -> PathBuf {
    dir.join(format!("{:016x}-{}.cix", root_key(root), tag))
}

/// The persisted-index identity tag for an HTTP embedder: `http-<sanitised model>-<dim>` (CPE-1273). A
/// distinct model or vector dimensionality yields a distinct tag → a distinct on-disk index file → a
/// clean rebuild when the user switches. The model is sanitised to a filename-safe slug (non-alphanumeric
/// → `-`, trimmed) so any model id produces a valid filename.
pub fn http_embedder_tag(model: &str, dim: usize) -> String {
    let slug: String = model
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "model" } else { slug };
    format!("http-{slug}-{dim}")
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
    progress: impl FnMut(ContentIndexProgress),
) -> Result<(SemanticIndex, ContentIndexBuildStats), String> {
    build_index_impl(root, cancel, progress, make_embedder())
}

/// [`build_index_with`] but backed by a caller-supplied `embedder` (CPE-1273) — the real HTTP embedder
/// path. Identical walk; only the embedder that turns each file's text into vectors differs. The
/// resulting index's persisted path must be keyed by the embedder's identity (see [`save_index_tagged`]).
pub fn build_index_with_embedder(
    root: &str,
    cancel: &AtomicBool,
    progress: impl FnMut(ContentIndexProgress),
    embedder: Box<dyn Embedder>,
) -> Result<(SemanticIndex, ContentIndexBuildStats), String> {
    build_index_impl(root, cancel, progress, embedder)
}

/// Shared walk used by both [`build_index_with`] (default FakeEmbedder) and [`build_index_with_embedder`]
/// (a configured real embedder). The only difference between the two is which `embedder` sizes + fills
/// the [`SemanticIndex`].
fn build_index_impl(
    root: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(ContentIndexProgress),
    embedder: Box<dyn Embedder>,
) -> Result<(SemanticIndex, ContentIndexBuildStats), String> {
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }
    let mut index = SemanticIndex::new(embedder);
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
            // CPE-1274: dispatches by extension — plain text/code unchanged, PDF/DOCX/XLSX/PPTX get
            // their actual text extracted instead of being NUL-sniffed out as "binary". `None` covers
            // everything the old inline check did (binary/unknown) plus a malformed/encrypted document
            // of a supported type — either way, skip this one file without failing the whole build.
            let Some(text) = crate::content_text::content_text_of(&path, &bytes) else {
                stats.files_skipped += 1;
                continue;
            };
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

/// Persist `index` to `dir/<hash(root)>.cix` (the default/local path), creating `dir` if needed.
pub fn save_index(index: &SemanticIndex, dir: &Path, root: &str) -> Result<(), String> {
    save_at(index, &index_path(dir, root))
}

/// Persist `index` to the embedder-identity-keyed path `dir/<hash(root)>-<tag>.cix` (CPE-1273), creating
/// `dir` if needed — so an index built with a given real embedder is stored separately from the local
/// one and from any other model's index.
pub fn save_index_tagged(index: &SemanticIndex, dir: &Path, root: &str, tag: &str) -> Result<(), String> {
    save_at(index, &index_path_for(dir, root, tag))
}

/// Shared persistence: create the parent dir, then atomically write the index at `path`.
fn save_at(index: &SemanticIndex, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    index.save(path).map_err(|e| e.to_string())
}

/// Load `root`'s persisted content index from `dir`, using the fixed [`CONTENT_INDEX_DIM`] embedder.
///
/// - No file yet → `Ok(None)` — a clean "needs build" signal, not an error (CPE-1262 acceptance).
/// - A file that exists but fails to parse (stale format, or a dimension mismatch from a different/older
///   embedder) → `Err` with a readable message — never a panic, mirroring [`SemanticIndex::load`]'s own
///   discipline.
pub fn load_index(dir: &Path, root: &str) -> Result<Option<SemanticIndex>, String> {
    load_at(&index_path(dir, root), make_embedder())
}

/// Load `root`'s index built with the real embedder identified by `tag`, attaching `embedder` (whose
/// `dim` must match the persisted vectors' — a mismatch degrades to a clean "rebuild it" `Err`, never a
/// panic, exactly as the default path). A missing file → `Ok(None)` ("needs build"). See [`load_at`].
pub fn load_index_tagged(
    dir: &Path,
    root: &str,
    tag: &str,
    embedder: Box<dyn Embedder>,
) -> Result<Option<SemanticIndex>, String> {
    load_at(&index_path_for(dir, root, tag), embedder)
}

/// Shared load: a missing file is a clean `Ok(None)` ("needs build"); a present-but-unloadable file
/// (stale format, or a dimension mismatch from a different/older embedder) is a readable `Err`, never a
/// panic — mirroring [`SemanticIndex::load`]'s discipline (CPE-1262 acceptance).
fn load_at(path: &Path, embedder: Box<dyn Embedder>) -> Result<Option<SemanticIndex>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    SemanticIndex::load(path, embedder)
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
    search_loaded(load_index(dir, root)?, query, k)
}

/// [`search_index`] against `root`'s index built with the real embedder identified by `tag`, embedding
/// the query with the supplied `embedder` (CPE-1273). No such index → a clean `index_exists: false`
/// outcome ("needs build"), same as the default path.
pub fn search_index_tagged(
    dir: &Path,
    root: &str,
    query: &str,
    k: usize,
    embedder: Box<dyn Embedder>,
    tag: &str,
) -> Result<ContentSearchOutcome, String> {
    search_loaded(load_index_tagged(dir, root, tag, embedder)?, query, k)
}

/// Shared scoring over an already-loaded index (or `None` → a clean empty, `index_exists: false`
/// outcome). The query is embedded by the index's own attached embedder inside [`SemanticIndex::search`].
fn search_loaded(
    index: Option<SemanticIndex>,
    query: &str,
    k: usize,
) -> Result<ContentSearchOutcome, String> {
    let Some(index) = index else {
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

// ---- Configured-embedder selection + command-layer entry points (CPE-1273) -------------------------

/// A configured **non-default** embedder plus the identity `tag` its persisted index is keyed by — the
/// `Some` arm of [`resolve_configured_embedder`]. (`None` there means "use the local FakeEmbedder path".)
pub type ResolvedEmbedder = (Box<dyn Embedder>, String);

/// Build the embedder a content-index command should use, from the persisted [`ContentEmbedderConfig`]
/// plus the API key the app fetched from the OS keychain (never from settings). Returns:
///
/// - `Ok(None)` — use the built-in local [`FakeEmbedder`] path (the legacy untagged index). This is the
///   disabled/unconfigured default, byte-identical to pre-CPE-1273.
/// - `Ok(Some((embedder, tag)))` — use the tagged index keyed by `tag` (the configured real embedder).
/// - `Err(_)` — enabled + configured but the endpoint is unreachable / the key is bad / the response is
///   malformed (or this build lacks the `http-embedder` feature). A clear error, never a panic; the
///   command surfaces it to the user as a failed build/search.
///
/// The `api_key` is consumed (moved into the embedder's `Authorization` header) and never returned.
pub fn resolve_configured_embedder(
    config: Option<&ContentEmbedderConfig>,
    api_key: Option<String>,
) -> Result<Option<ResolvedEmbedder>, String> {
    let Some(c) = config else { return Ok(None) };
    if !c.enabled || c.base_url.trim().is_empty() || c.model.trim().is_empty() {
        return Ok(None);
    }
    #[cfg(feature = "http-embedder")]
    {
        let emb = crate::http_embedder::HttpEmbedder::connect(c.base_url.trim(), c.model.trim(), api_key)?;
        let tag = http_embedder_tag(c.model.trim(), emb.dim());
        Ok(Some((Box::new(emb), tag)))
    }
    #[cfg(not(feature = "http-embedder"))]
    {
        let _ = api_key;
        Err("this build was compiled without HTTP-embedder support".to_string())
    }
}

/// Build `root`'s content index with the embedder selected by `config` + `api_key`, then persist it to
/// the matching path (tagged for a real embedder, untagged for the local default) — but only if the build
/// wasn't cancelled mid-walk (a superseded build must not overwrite a newer one's save). The one entry
/// point the `content_index_build` command dispatches into, keeping that command thin.
pub fn build_and_save(
    root: &str,
    dir: &Path,
    cancel: &AtomicBool,
    progress: impl FnMut(ContentIndexProgress),
    config: Option<&ContentEmbedderConfig>,
    api_key: Option<String>,
) -> Result<ContentIndexBuildStats, String> {
    match resolve_configured_embedder(config, api_key)? {
        Some((embedder, tag)) => {
            let (index, stats) = build_index_with_embedder(root, cancel, progress, embedder)?;
            if !cancel.load(Ordering::Relaxed) {
                save_index_tagged(&index, dir, root, &tag)?;
            }
            Ok(stats)
        }
        None => {
            let (index, stats) = build_index_with(root, cancel, progress)?;
            if !cancel.load(Ordering::Relaxed) {
                save_index(&index, dir, root)?;
            }
            Ok(stats)
        }
    }
}

/// Search `root`'s content index with the embedder selected by `config` + `api_key`. The one entry point
/// the `content_search` command dispatches into. A disabled/unconfigured `config` searches the local
/// index (byte-identical to pre-CPE-1273); an enabled+unreachable endpoint is a clear `Err`.
pub fn search_configured(
    dir: &Path,
    root: &str,
    query: &str,
    k: usize,
    config: Option<&ContentEmbedderConfig>,
    api_key: Option<String>,
) -> Result<ContentSearchOutcome, String> {
    match resolve_configured_embedder(config, api_key)? {
        Some((embedder, tag)) => search_index_tagged(dir, root, query, k, embedder, &tag),
        None => search_index(dir, root, query, k),
    }
}

/// Probe a configured endpoint (embed a short string) and return the detected vector dimensionality, or
/// a clear error — the backend of the Settings "Test connection" button (CPE-1273). Ignores the
/// `enabled` flag (the user tests before flipping it on). Feature-gated; without `http-embedder` this is
/// a compile-time no-op (the app only calls it in the feature-on build).
#[cfg(feature = "http-embedder")]
pub fn probe_embedder(base_url: &str, model: &str, api_key: Option<String>) -> Result<usize, String> {
    if base_url.trim().is_empty() || model.trim().is_empty() {
        return Err("enter an endpoint URL and a model name first".to_string());
    }
    Ok(crate::http_embedder::HttpEmbedder::connect(base_url.trim(), model.trim(), api_key)?.dim())
}

/// A short snippet of `path`'s own text, centred on `query`'s first literally-matching token when one is
/// found (case-insensitive), else the start of the file. Best-effort: an unreadable/moved/binary file
/// yields an empty snippet rather than an error — the file could have changed since the index was built.
///
/// CPE-1274: re-extracts through the same [`crate::content_text::content_text_of`] dispatch the build
/// walk uses, rather than raw-`from_utf8_lossy`-ing the file's bytes — a PDF/DOCX/XLSX/PPTX hit's
/// snippet is real extracted text (or empty, for an extraction failure), never mojibake from decoding
/// a ZIP/binary container as if it were UTF-8 text.
fn snippet_for(path: &str, query: &str) -> String {
    let Ok(bytes) = std::fs::read(path) else { return String::new() };
    let Some(text) = crate::content_text::content_text_of(Path::new(path), &bytes) else {
        return String::new();
    };
    let flat: Vec<char> = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect();
    if flat.is_empty() {
        return String::new();
    }
    // Lowercase **per-char, positionally** so the lowered sequence stays exactly `flat.len()` long and
    // index-aligned with `flat`. Whole-string `str::to_lowercase()` can expand some characters into
    // multiple code points (e.g. Turkish 'İ' U+0130 → "i" + a combining dot above), which desyncs a
    // char-count taken from the lowered string from an index into the original `flat` — the review repro
    // (CPE-1262) hit exactly this: a hit char index past `flat.len()` made `start > end` after clamping
    // `end`, and `flat[start..end]` panicked. Taking only the first lowered char per position is a
    // best-effort case-fold (not full Unicode case-folding), but it guarantees every index found below is
    // a valid, in-bounds index into `flat` — no panic is possible regardless of the input's scripts.
    let lower_chars: Vec<char> = flat.iter().map(|&c| c.to_lowercase().next().unwrap_or(c)).collect();
    let hit_char = query.split_whitespace().find_map(|term| {
        let term_chars: Vec<char> = term.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect();
        if term_chars.is_empty() || term_chars.len() > lower_chars.len() {
            return None;
        }
        lower_chars.windows(term_chars.len()).position(|w| w == term_chars.as_slice())
    });
    // `hit_char` (when `Some`) is guaranteed < flat.len() by construction (it's a valid `windows()`
    // position over a slice the same length as `flat`); `.min(flat.len())` is a belt-and-braces guard in
    // case that invariant is ever weakened by a future edit, not load-bearing today.
    let start = hit_char.map(|c| c.saturating_sub(SNIPPET_MAX_CHARS / 4)).unwrap_or(0).min(flat.len());
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

    /// Regression (review of CPE-1262, PR #561): `snippet_for` must never panic on text containing
    /// characters whose `str::to_lowercase()` expands into MORE code points than the original char count
    /// — e.g. Turkish 'İ' (U+0130 LATIN CAPITAL LETTER I WITH DOT ABOVE) lowercases to "i" + a combining
    /// dot above (two chars for one). The old implementation lowercased the whole string at once and used
    /// `lower[..byte_idx].chars().count()` as an index into the ORIGINAL (unexpanded) `flat` char vector —
    /// with enough expanding chars before a match, that index landed past `flat.len()`, making
    /// `start > end` after `end` was clamped, and `flat[start..end]` panicked (confirmed repro:
    /// `flat.len()=101 lower.chars=201 hit_char=Some(200) start=140 end=101`). A panic inside
    /// `snippet_for` propagates out of `spawn_blocking` in the `content_search` command as a generic
    /// "task panicked" `Err`, breaking search for any file containing ordinary Turkish text (e.g.
    /// "İstanbul", "İş") that happens to match a query. This test pins the exact repro shape (a long run
    /// of 'İ' before the matched term) end-to-end through `search_index` (the real product path).
    #[test]
    fn search_index_does_not_panic_on_case_expanding_characters_before_a_match() {
        let tree = scratch("turkish-tree");
        // A long run of 'İ' (each lowercases to 2 chars) before the query term reproduces the review's
        // repro shape: enough expansion that a byte-derived lowercase char-count runs past flat.len().
        let prefix = "İ".repeat(80);
        let content = format!("{prefix} volcano eruption near Istanbul today");
        fs::write(tree.join("turkish.txt"), content.as_bytes()).unwrap();
        let idxdir = scratch("turkish-idx");
        let root = tree.to_string_lossy().to_string();

        let (index, _) = build_index(&root, &AtomicBool::new(false)).unwrap();
        save_index(&index, &idxdir, &root).unwrap();

        // Must return Ok, never panic (never a generic "task panicked" Err either) — this is the exact
        // path the review's repro broke: search_index -> ContentHit -> snippet_for.
        let outcome = search_index(&idxdir, &root, "volcano eruption", 5).unwrap();
        assert!(outcome.index_exists);
        assert!(!outcome.hits.is_empty(), "the Turkish-prefixed doc should still match the query");
        assert!(!outcome.hits[0].snippet.is_empty(), "a non-empty snippet should come back, not a panic");

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    /// Same regression, exercised directly against `snippet_for` (the function that actually panicked) —
    /// with the match term appearing AFTER a run of case-expanding characters, so a naive whole-string
    /// lowercase-then-char-count would compute an index past `flat.len()`. Also checks the snippet is
    /// centred near the real match rather than silently falling back to the start of the file.
    #[test]
    fn snippet_for_stays_index_aligned_with_case_expanding_characters() {
        let tree = scratch("snippet-turkish");
        let prefix = "İ".repeat(200); // enough expansion that the old bug's `start` would exceed `flat.len()`
        let content = format!("{prefix} needle-term-here");
        let file = tree.join("t.txt");
        fs::write(&file, content.as_bytes()).unwrap();

        // Never panics, regardless of how many case-expanding chars precede the match.
        let snippet = snippet_for(&file.to_string_lossy(), "needle-term-here");
        assert!(snippet.contains("needle-term-here"), "snippet should include the actual match: {snippet:?}");

        let _ = fs::remove_dir_all(&tree);
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

    // ---- configurable real embedder selection + tagged index plumbing (CPE-1273) ----------------

    #[test]
    fn resolve_returns_none_when_disabled_or_unconfigured() {
        // No config at all → local FakeEmbedder path.
        assert!(resolve_configured_embedder(None, None).unwrap().is_none());
        // Explicitly disabled.
        let disabled = ContentEmbedderConfig { enabled: false, base_url: "http://x/v1".into(), model: "m".into() };
        assert!(resolve_configured_embedder(Some(&disabled), None).unwrap().is_none());
        // Enabled but missing url/model → still the local path (not an error).
        let no_url = ContentEmbedderConfig { enabled: true, base_url: "  ".into(), model: "m".into() };
        assert!(resolve_configured_embedder(Some(&no_url), None).unwrap().is_none());
        let no_model = ContentEmbedderConfig { enabled: true, base_url: "http://x/v1".into(), model: "".into() };
        assert!(resolve_configured_embedder(Some(&no_model), None).unwrap().is_none());
    }

    /// Without the `http-embedder` feature, an enabled+configured selection is a clear compile-time-gated
    /// error, never a panic. (With the feature on, this path instead tries to reach the endpoint — which
    /// needs the user's server and so isn't exercised in a no-network unit test.)
    #[cfg(not(feature = "http-embedder"))]
    #[test]
    fn resolve_enabled_without_feature_is_a_clear_err() {
        let cfg = ContentEmbedderConfig { enabled: true, base_url: "http://x/v1".into(), model: "m".into() };
        assert!(resolve_configured_embedder(Some(&cfg), None).is_err());
    }

    #[test]
    fn http_embedder_tag_is_filename_safe_and_identity_keyed() {
        let a = http_embedder_tag("text-embedding-3-small", 1536);
        assert_eq!(a, "http-text-embedding-3-small-1536");
        // Different model or dim ⇒ different tag (⇒ different index file ⇒ a switch rebuilds).
        assert_ne!(a, http_embedder_tag("nomic-embed-text", 1536));
        assert_ne!(a, http_embedder_tag("text-embedding-3-small", 768));
        // Awkward characters (slashes, colons, spaces) are slugified to a safe filename fragment.
        let slug = http_embedder_tag("Qwen/Qwen3-Embedding:0.6b", 1024);
        assert!(slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "tag must be filename-safe: {slug}");
        // An all-symbol model name still yields a usable tag.
        assert_eq!(http_embedder_tag("!!!", 8), "http-model-8");
    }

    #[test]
    fn tagged_index_path_is_distinct_from_default_and_per_tag() {
        let dir = PathBuf::from("Z:/appdata/content-index");
        let root = "Z:/repos/one";
        assert_ne!(index_path(&dir, root), index_path_for(&dir, root, "http-m-1024"));
        assert_ne!(
            index_path_for(&dir, root, "http-a-1024"),
            index_path_for(&dir, root, "http-b-1024"),
        );
        assert_eq!(index_path_for(&dir, root, "t"), index_path_for(&dir, root, "t"));
    }

    /// The tagged build→persist→search round-trip works with ANY `Embedder`, proving the real-embedder
    /// plumbing end-to-end without a network: here a FakeEmbedder at a DIFFERENT dim stands in for "a
    /// different model". The tagged file is written; the default (untagged) file is NOT.
    #[test]
    fn tagged_build_persist_search_round_trips_with_a_custom_embedder() {
        let tree = scratch("tagged-tree");
        sample_tree(&tree);
        let idxdir = scratch("tagged-idx");
        let root = tree.to_string_lossy().to_string();
        let tag = "http-fake-256";

        let embedder: Box<dyn Embedder> = Box::new(FakeEmbedder::new(256));
        let (index, stats) =
            build_index_with_embedder(&root, &AtomicBool::new(false), |_| {}, embedder).unwrap();
        assert_eq!(stats.files_indexed, 2);
        save_index_tagged(&index, &idxdir, &root, tag).unwrap();

        // Only the tagged file exists — the default local path was never written.
        assert!(index_path_for(&idxdir, &root, tag).is_file());
        assert!(!index_path(&idxdir, &root).exists());

        let outcome =
            search_index_tagged(&idxdir, &root, "quick fox", 5, Box::new(FakeEmbedder::new(256)), tag).unwrap();
        assert!(outcome.index_exists);
        assert!(outcome.hits[0].path.replace('\\', "/").ends_with("fox.txt"));

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    /// Switching embedders (a different tag) reads as a clean "needs build", NOT a stale error or a
    /// crash: the new tag points at a file that doesn't exist yet → `index_exists: false`.
    #[test]
    fn switching_embedder_tag_reads_as_needs_build() {
        let tree = scratch("switch-tree");
        sample_tree(&tree);
        let idxdir = scratch("switch-idx");
        let root = tree.to_string_lossy().to_string();

        // Build + persist under tag A.
        let (index, _) =
            build_index_with_embedder(&root, &AtomicBool::new(false), |_| {}, Box::new(FakeEmbedder::new(256))).unwrap();
        save_index_tagged(&index, &idxdir, &root, "http-modelA-256").unwrap();

        // Searching under a DIFFERENT tag (a switched model) finds no index → needs build, no error.
        let switched =
            search_index_tagged(&idxdir, &root, "quick fox", 5, Box::new(FakeEmbedder::new(256)), "http-modelB-256").unwrap();
        assert!(!switched.index_exists);
        assert!(switched.hits.is_empty());

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    #[test]
    fn build_and_save_then_search_configured_with_no_config_uses_the_local_untagged_index() {
        let tree = scratch("cfg-none-tree");
        sample_tree(&tree);
        let idxdir = scratch("cfg-none-idx");
        let root = tree.to_string_lossy().to_string();

        // No config → the local FakeEmbedder + the legacy untagged path (byte-identical to pre-CPE-1273).
        let stats = build_and_save(&root, &idxdir, &AtomicBool::new(false), |_| {}, None, None).unwrap();
        assert_eq!(stats.files_indexed, 2);
        assert!(index_path(&idxdir, &root).is_file(), "the default untagged index must be written");

        let outcome = search_configured(&idxdir, &root, "quick fox", 5, None, None).unwrap();
        assert!(outcome.index_exists);
        assert!(outcome.hits[0].path.replace('\\', "/").ends_with("fox.txt"));

        // A disabled config behaves exactly like no config.
        let disabled = ContentEmbedderConfig::default();
        let outcome2 = search_configured(&idxdir, &root, "quick fox", 5, Some(&disabled), None).unwrap();
        assert_eq!(outcome.hits[0].path, outcome2.hits[0].path);

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
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
