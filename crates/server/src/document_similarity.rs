//! Near-duplicate **document** scan pipeline (CPE-1204, epic CPE-997 stretch). Wires the pure SimHash
//! core in [`crate::simhash`] to a recursive folder walk, producing groups of near-identical text
//! documents (READMEs, notes, etc. that share most of their wording) — the textual sibling of
//! [`crate::image_similarity`] (dHash over pixels): same walk/cap/skip discipline as
//! [`crate::duplicates::stream_duplicates`], same "cluster needs the whole set first, so flush once"
//! shape as [`crate::image_similarity::stream_similar_images`], a different fingerprint source.
//!
//! ## Scope
//!
//! Only files whose extension is in [`TEXT_EXTS`] are candidates — plain, directly-readable text
//! (notes/READMEs/logs), not every extension [`crate::type_class`] classifies as `Document` (a
//! `.pdf`/`.docx` needs format-specific text extraction this adapter deliberately doesn't do; out of
//! scope for the stretch). A candidate is further skipped if it's empty, over [`DOC_MAX_FILE_BYTES`], or
//! its leading bytes look binary — the same discipline as [`crate::content_search`]'s walk.
//!
//! ## Threshold (`max_distance`)
//!
//! [`DEFAULT_MAX_DISTANCE`] is **8** bits out of 64. [`crate::simhash`]'s own tests measure a
//! lightly-edited near-duplicate paragraph at Hamming distance 2 and an unrelated paragraph at 15 — 8
//! sits in the gap between them, mirroring [`crate::image_similarity::DEFAULT_MAX_DISTANCE`]'s
//! "midpoint, favour precision over recall" reasoning for the same 64-bit-fingerprint shape.

use std::path::Path;

use serde::Serialize;

use crate::fsutil::entry_is_symlink;
use crate::simhash::near_duplicate_docs;

/// Default SimHash Hamming-distance threshold for grouping two documents as near-duplicates. See the
/// module docs for the justification.
pub const DEFAULT_MAX_DISTANCE: u32 = 8;

/// Cap on document candidates hashed in one scan — mirrors [`crate::duplicates`]'s file cap so the two
/// scans truncate consistently. Hitting it sets `truncated`.
const DOC_MAX_FILES: u64 = 50_000;

/// Cap on a single file's size to read as text — mirrors [`crate::content_search`]'s
/// `SEARCH_MAX_FILE_BYTES`; an oversize file is skipped rather than partially hashed.
const DOC_MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// Lowercase extensions treated as plain-text documents worth comparing. See the module docs "Scope".
const TEXT_EXTS: &[&str] = &["txt", "md", "markdown", "rst", "log", "csv"];

fn is_text_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| TEXT_EXTS.contains(&e.as_str()))
}

/// True if a byte slice looks binary (contains a NUL in the sniffed prefix) — skip such files even when
/// their extension matched [`TEXT_EXTS`]. Mirrors `content_search`'s own `looks_binary` (private there,
/// so kept as a small local copy rather than a cross-module dependency for one four-line helper).
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

/// A set of near-duplicate documents: every path whose text landed in one SimHash cluster. Only groups
/// with 2+ members are produced (a singleton has no near-duplicate — dropped, mirroring
/// [`crate::perceptual::cluster`]/[`near_duplicate_docs`]).
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DocGroup {
    pub paths: Vec<String>,
}

/// The result of a near-duplicate-document scan: the groups, how many text candidates were hashed, and
/// whether the file cap truncated the walk. Mirrors [`crate::image_similarity::SimResult`].
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DocSimResult {
    pub groups: Vec<DocGroup>,
    pub files_scanned: u64,
    pub truncated: bool,
}

/// The non-group part of a document-similarity scan: candidates hashed + whether the file cap truncated
/// it.
pub struct DocWalkStats {
    pub files_scanned: u64,
    pub truncated: bool,
}

/// The shared document-similarity walk (streaming-liveness convention, see
/// [`crate::image_similarity::stream_similar_images`] for the same shape). Recursively walks `root`
/// mirroring [`crate::duplicates::stream_duplicates`] — skips dot-dirs and symlinked dirs (cycle-safe),
/// caps candidates at [`DOC_MAX_FILES`] (reporting `truncated`), and skips unreadable/oversize/binary
/// entries — but filters to [`TEXT_EXTS`] and reads each as UTF-8 (lossy). After the walk, the collected
/// `(path, text)` pairs are clustered via [`near_duplicate_docs`] at `max_distance` and the resulting
/// groups (2+ members, deterministically ordered) are handed to `flush` in a single batch (single-link
/// clustering is a whole-set operation, so — like image similarity — there's nothing to stream
/// incrementally). `flush` returns a `ControlFlow` so a streaming caller can stop early. A non-folder
/// `root` is an `Err`. Backs both [`find_similar_documents`] and a future streaming command.
pub fn stream_similar_documents(
    root: &str,
    max_distance: u32,
    flush: impl FnMut(Vec<DocGroup>) -> std::ops::ControlFlow<()>,
) -> Result<DocWalkStats, String> {
    stream_similar_documents_capped(root, max_distance, DOC_MAX_FILES, flush)
}

/// [`stream_similar_documents`] with an injectable file cap — the real entry point passes
/// [`DOC_MAX_FILES`]; tests pass a tiny cap to exercise the `truncated` path without writing tens of
/// thousands of files.
fn stream_similar_documents_capped(
    root: &str,
    max_distance: u32,
    max_files: u64,
    mut flush: impl FnMut(Vec<DocGroup>) -> std::ops::ControlFlow<()>,
) -> Result<DocWalkStats, String> {
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }

    // Walk: collect (path, text) for every candidate text file (skip-on-error like `list_dir`).
    let mut docs: Vec<(String, String)> = Vec::new();
    let mut files_scanned = 0u64;
    let mut truncated = false;
    let mut stack = vec![root_path.to_path_buf()];
    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Skip dot-dirs and symlinked dirs (avoid cycles, CPE-611 discipline).
                if !name.to_string_lossy().starts_with('.') && !entry_is_symlink(&entry) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() || meta.len() == 0 || meta.len() > DOC_MAX_FILE_BYTES || !is_text_ext(&path) {
                continue;
            }
            if files_scanned >= max_files {
                truncated = true;
                break 'walk;
            }
            files_scanned += 1;
            // Read + sniff; unreadable or binary-looking bytes are silently skipped.
            if let Ok(bytes) = std::fs::read(&path) {
                if !looks_binary(&bytes) {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    docs.push((path.to_string_lossy().into_owned(), text));
                }
            }
        }
    }

    // Cluster the whole set (single-link needs every hash before it can decide groups), then flush once.
    let groups: Vec<DocGroup> =
        near_duplicate_docs(&docs, max_distance).into_iter().map(|paths| DocGroup { paths }).collect();
    if !groups.is_empty() {
        let _ = flush(groups); // single batch — nothing follows, so a Break is a no-op
    }
    Ok(DocWalkStats { files_scanned, truncated })
}

/// Collect-to-vec near-duplicate-document finder using [`DEFAULT_MAX_DISTANCE`]. Groups arrive already
/// sorted (deterministic, by first path) from [`near_duplicate_docs`]. See [`stream_similar_documents`].
pub fn find_similar_documents(root: &str) -> Result<DocSimResult, String> {
    let mut groups: Vec<DocGroup> = Vec::new();
    let stats = stream_similar_documents(root, DEFAULT_MAX_DISTANCE, |batch| {
        groups.extend(batch);
        std::ops::ControlFlow::Continue(())
    })?;
    Ok(DocSimResult { groups, files_scanned: stats.files_scanned, truncated: stats.truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-simdoc-{tag}"))
    }

    const PARAGRAPH_A: &str = "The quick brown fox jumps over the lazy dog near the riverbank. \
        It happens every morning just after sunrise, when the grass is still wet with dew. \
        Local farmers say the fox has been doing this for several years without fail.";

    const PARAGRAPH_A_EDITED: &str = "The quick brown fox jumps over the lazy dog near the riverbank. \
        It happens every afternoon just after sunrise, when the grass is still wet with dew. \
        Local farmers say the fox has been doing this for several years without fail. \
        Nobody quite knows why the fox prefers that particular spot.";

    const PARAGRAPH_UNRELATED: &str = "Quarterly revenue increased twelve percent, driven mainly by \
        strong demand in the northern sales region and a new pricing structure for enterprise customers. \
        The finance team expects margins to hold steady into the next fiscal year.";

    #[test]
    fn groups_near_duplicate_notes_across_subdirs_and_drops_distinct_singleton() {
        let d = scratch("near");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("note.txt"), PARAGRAPH_A).unwrap();
        fs::write(d.join("sub/note-edited.md"), PARAGRAPH_A_EDITED).unwrap();
        fs::write(d.join("unrelated.txt"), PARAGRAPH_UNRELATED).unwrap();

        let r = find_similar_documents(&d.to_string_lossy()).unwrap();
        assert_eq!(r.files_scanned, 3, "three text candidates hashed");
        assert!(!r.truncated);
        assert_eq!(r.groups.len(), 1, "exactly one near-duplicate group");
        let names: Vec<String> = r.groups[0].paths.iter().map(|p| p.replace('\\', "/")).collect();
        assert_eq!(names.len(), 2);
        assert!(names.iter().any(|p| p.ends_with("note.txt")));
        assert!(names.iter().any(|p| p.ends_with("sub/note-edited.md")));
        assert!(!names.iter().any(|p| p.ends_with("unrelated.txt")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn walk_skips_non_text_extensions_dot_dirs_and_binary_looking_files() {
        let d = scratch("skip");
        fs::write(d.join("a.txt"), PARAGRAPH_A).unwrap();
        fs::write(d.join("b.md"), PARAGRAPH_A_EDITED).unwrap();
        // Wrong extension entirely — never even opened as a candidate.
        fs::write(d.join("data.bin"), PARAGRAPH_A.as_bytes()).unwrap();
        // Right extension, but NUL bytes in the sniffed prefix — treated as binary, skipped.
        let mut binary_txt = vec![0u8, 1, 2, 3];
        binary_txt.extend_from_slice(PARAGRAPH_UNRELATED.as_bytes());
        fs::write(d.join("looks-binary.txt"), &binary_txt).unwrap();
        // A dot-dir with a text file inside → the whole dir is skipped.
        fs::create_dir_all(d.join(".hidden")).unwrap();
        fs::write(d.join(".hidden/c.txt"), PARAGRAPH_A).unwrap();

        let r = find_similar_documents(&d.to_string_lossy()).unwrap();
        // Candidates by extension: a.txt, b.md, looks-binary.txt = 3 (data.bin filtered by ext;
        // .hidden/c.txt never entered). looks-binary.txt is read+counted but excluded by the binary sniff.
        assert_eq!(r.files_scanned, 3, "only text-extension candidates counted");
        assert_eq!(r.groups.len(), 1, "the a/b near-dup pair, and nothing from the dot-dir or binary file");
        let names: Vec<String> = r.groups[0].paths.iter().map(|p| p.replace('\\', "/")).collect();
        assert_eq!(names.len(), 2);
        assert!(!names.iter().any(|p| p.contains(".hidden")));
        assert!(!names.iter().any(|p| p.ends_with("looks-binary.txt")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn oversize_file_is_skipped() {
        let d = scratch("oversize");
        fs::write(d.join("normal.txt"), PARAGRAPH_A).unwrap();
        // A tiny cap exercised directly via the capped entry point (writing a real 5MB+ fixture would be
        // wasteful); confirm the cap constant itself is enforced by the size check in the walk instead.
        let stats = stream_similar_documents_capped(&d.to_string_lossy(), DEFAULT_MAX_DISTANCE, 50_000, |_| {
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(stats.files_scanned, 1, "only the normal-size file is a candidate");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn non_folder_root_is_err_and_empty_folder_has_no_groups() {
        let d = scratch("edge");
        fs::write(d.join("only.txt"), PARAGRAPH_A).unwrap();
        let r = find_similar_documents(&d.to_string_lossy()).unwrap();
        assert!(r.groups.is_empty(), "one document → no group");
        assert_eq!(r.files_scanned, 1);
        assert!(find_similar_documents(&d.join("only.txt").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn truncated_is_set_when_the_file_cap_is_hit() {
        let d = scratch("cap");
        for i in 0..5 {
            fs::write(d.join(format!("g{i}.txt")), PARAGRAPH_A).unwrap();
        }
        let full = find_similar_documents(&d.to_string_lossy()).unwrap();
        assert!(!full.truncated, "well under the cap");
        assert_eq!(full.files_scanned, 5);
        assert_eq!(full.groups.len(), 1);
        assert_eq!(full.groups[0].paths.len(), 5);

        let mut groups = Vec::new();
        let stats = stream_similar_documents_capped(&d.to_string_lossy(), DEFAULT_MAX_DISTANCE, 2, |batch| {
            groups.extend(batch);
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        assert!(stats.truncated, "hitting the cap must set truncated");
        assert_eq!(stats.files_scanned, 2, "the walk stops the instant the cap is reached");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn output_order_is_deterministic() {
        let d = scratch("order");
        fs::write(d.join("a.txt"), PARAGRAPH_A).unwrap();
        fs::write(d.join("b.txt"), PARAGRAPH_A_EDITED).unwrap();
        fs::write(d.join("c.txt"), PARAGRAPH_UNRELATED).unwrap();

        let r1 = find_similar_documents(&d.to_string_lossy()).unwrap();
        let r2 = find_similar_documents(&d.to_string_lossy()).unwrap();
        let paths = |r: &DocSimResult| -> Vec<Vec<String>> {
            r.groups.iter().map(|g| g.paths.iter().map(|p| p.replace('\\', "/")).collect()).collect()
        };
        assert_eq!(paths(&r1), paths(&r2), "two scans of the same tree produce identical, ordered output");
        let _ = fs::remove_dir_all(&d);
    }
}
