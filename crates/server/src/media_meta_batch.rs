//! Batch metadata apply (CPE-949, epic CPE-725): apply one shared set of [`MetaEdit`]s to a whole
//! **selection** of files (each carrying its own current fields), returning a per-file result and a run
//! summary. Builds on [`crate::media_meta_edit::apply_edits`] — the studio's "edit these 40 photos at
//! once" core. Pure; the codec layer reads/writes the files.

use crate::media_meta_edit::{apply_edits, EditResult, MetaEdit, MetaField};

/// A file in the selection with its current metadata fields.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FileMeta {
    pub path: String,
    pub fields: Vec<MetaField>,
}

/// The result of applying the shared edits to one file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FileEditResult {
    pub path: String,
    pub result: EditResult,
}

/// A run summary across the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BatchSummary {
    pub files: usize,
    /// Files where at least one edit applied.
    pub changed: usize,
    /// Files where nothing applied (all edits were rejected or no-ops for that file).
    pub unchanged: usize,
    /// Total rejected edits across all files (e.g. read-only field targets).
    pub rejected: usize,
}

/// Apply the same `edits` to every file in `files`, preserving order.
pub fn apply_batch(files: &[FileMeta], edits: &[MetaEdit]) -> Vec<FileEditResult> {
    files
        .iter()
        .map(|f| FileEditResult { path: f.path.clone(), result: apply_edits(&f.fields, edits) })
        .collect()
}

/// Summarise per-file results into changed/unchanged/rejected counts.
pub fn summarize(results: &[FileEditResult]) -> BatchSummary {
    let mut s = BatchSummary { files: results.len(), ..Default::default() };
    for r in results {
        if r.result.applied.is_empty() {
            s.unchanged += 1;
        } else {
            s.changed += 1;
        }
        s.rejected += r.result.rejected.len();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(group: &str, key: &str, value: &str, editable: bool) -> MetaField {
        MetaField { group: group.into(), key: key.into(), value: value.into(), editable }
    }
    fn file(path: &str, fields: Vec<MetaField>) -> FileMeta {
        FileMeta { path: path.into(), fields }
    }
    fn set(group: &str, key: &str, value: &str) -> MetaEdit {
        MetaEdit::Set { group: group.into(), key: key.into(), value: value.into() }
    }

    #[test]
    fn applies_the_same_edit_to_every_file() {
        let files = vec![
            file("/1.jpg", vec![field("iptc", "Copyright", "old", true)]),
            file("/2.jpg", vec![]), // no such field yet → added
        ];
        let out = apply_batch(&files, &[set("iptc", "Copyright", "© 2026 Me")]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].result.fields[0].value, "© 2026 Me"); // updated
        assert_eq!(out[1].result.fields[0].value, "© 2026 Me"); // added
        assert!(out.iter().all(|r| !r.result.applied.is_empty()));
    }

    #[test]
    fn summary_counts_changed_unchanged_and_rejected() {
        let files = vec![
            file("/a.jpg", vec![field("iptc", "Caption", "x", true)]),      // editable → changed
            file("/b.jpg", vec![field("exif", "PixelWidth", "1920", false)]), // read-only target below
        ];
        // Two edits: one hits an editable field on /a (added on /b), one targets /b's read-only field.
        let edits = vec![set("iptc", "Caption", "hi"), set("exif", "PixelWidth", "1")];
        let out = apply_batch(&files, &edits);
        let s = summarize(&out);
        assert_eq!(s.files, 2);
        // /a: Caption set (applied) + PixelWidth added (applied) → changed.
        // /b: Caption added (applied) + PixelWidth rejected (read-only) → changed, 1 rejected.
        assert_eq!(s.changed, 2);
        assert_eq!(s.unchanged, 0);
        assert_eq!(s.rejected, 1);
    }

    #[test]
    fn empty_selection_summarises_to_zero() {
        let s = summarize(&apply_batch(&[], &[set("id3", "Artist", "x")]));
        assert_eq!(s, BatchSummary { files: 0, changed: 0, unchanged: 0, rejected: 0 });
    }
}
