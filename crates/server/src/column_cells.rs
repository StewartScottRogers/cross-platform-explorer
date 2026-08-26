//! Metadata-column cell extraction over real files (CPE-1145, epic CPE-707): the file-I/O layer on top
//! of [`crate::column_extract::extract_column`], which is pure-over-bytes and never touches disk. This
//! module reads a **capped header** per path and dispatches through it to produce a typed [`CellValue`]
//! per row, for the details-view column picker (CPE-1146 builds the UI on top of this).
//!
//! Header-only reads keep a column fill cheap enough to run per visible row even for a large media file.
//! An unreadable/undecodable file yields an empty cell rather than failing the whole batch — mirrors
//! `list_dir`'s and `content_search`'s skip-on-error ethos (never a partial job aborted by one bad file).
//! One shared walker ([`stream_column_cells`]) backs both the streaming command and its collect-to-vec
//! test/non-streaming variant ([`column_cells`]), per `docs/design/STREAMING.md`.

use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::column_extract::{extract_column, native_tags_cell, MetaColumn};
use crate::metadata_column::CellValue;
use crate::native_bridge::read_native_tags;

/// One pickable metadata column for the picker UI: a stable string id (for persistence in
/// [`crate::column_config`]), a friendly label, the typed [`MetaColumn`] to pass back into
/// [`column_cells`]/the streaming command, and the lowercase extensions it applies to (so the picker can
/// grey out a non-applicable row). An **empty** `extensions` list is the "applies to all files" sentinel
/// (CPE-1166): the column runs for every file and the picker must never grey it out.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AvailableColumn {
    pub id: String,
    pub label: String,
    pub column: MetaColumn,
    pub extensions: Vec<String>,
}

/// Every metadata column the details-view picker can offer, across all families (audio/image/doc/video).
pub fn available_columns() -> Vec<AvailableColumn> {
    MetaColumn::all()
        .into_iter()
        .map(|column| AvailableColumn {
            id: column.id(),
            label: column.label(),
            extensions: column.extensions().iter().map(|s| s.to_string()).collect(),
            column,
        })
        .collect()
}

/// A capped header read keeps a column fill cheap even for a huge media file. 1 MiB matches the bound
/// `media_meta_read::read_pdf` already applies internally to its own last-resort byte scan, so this is
/// the smallest cap that doesn't undercut an extractor that's already relying on it; the other
/// extractors (ID3/FLAC/OGG tags, image headers, the MP4 `moov` box when it's near the front of the
/// file) resolve from far less. A `moov` box placed at the end of the file (a non-"faststart" MP4) or a
/// PDF whose page objects live beyond 1 MiB won't be found — an accepted trade-off for a per-row column
/// fill (`DocPages`/`VideoDuration`/`VideoTag` are approximations for such files, not exhaustive parses).
const HEADER_CAP: u64 = 1_048_576;

/// Placeholder used when pre-formatting a cell's display string — an empty cell shows this.
const EMPTY_PLACEHOLDER: &str = "—";

/// How many cells to buffer before flushing a batch to a streaming caller.
pub const COLUMN_CELLS_BATCH: usize = 64;

/// One streamed/collected cell result: the source path, the typed value (so the frontend can do
/// type-aware behaviour later, e.g. right-aligning numbers), and a pre-formatted display string —
/// reusing [`CellValue::display`] rather than making the frontend reimplement byte/float/dimension
/// formatting in TypeScript.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MetadataCell {
    pub path: String,
    pub cell: CellValue,
    pub display: String,
}

/// Read a capped header of `path` (at most [`HEADER_CAP`] bytes), plus a `truncated` flag = whether the
/// file was LARGER than the cap (so bytes past the header were not read). Any I/O failure (missing file, no
/// permission, a directory, …) surfaces as `Err`; [`stream_column_cells`] below turns that into an empty
/// cell rather than aborting the batch. `truncated` lets the caller suppress a metadata value that a
/// truncated read could compute *wrongly* rather than merely miss (see the `DocPages` note there).
fn read_header(path: &Path) -> std::io::Result<(Vec<u8>, bool)> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    // Read one byte past the cap so we can tell "file exceeds the cap" from "file is exactly the cap".
    file.take(HEADER_CAP + 1).read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 > HEADER_CAP;
    if truncated {
        bytes.truncate(HEADER_CAP as usize);
    }
    Ok((bytes, truncated))
}

/// The shared column-cell walk (CPE-1145): for each of `paths`, read a capped header and extract `col`
/// via [`extract_column`], invoking `flush` with each batch of ≤`batch` [`MetadataCell`]s; `flush`
/// returns a `ControlFlow` so a streaming caller can stop early. An unreadable file, or one whose
/// extension/bytes don't match `col`, yields [`CellValue::Empty`] — never an `Err`, never a panic. Backs
/// both [`column_cells`] (collect) and the streaming command.
pub fn stream_column_cells(
    paths: &[String],
    col: MetaColumn,
    batch: usize,
    mut flush: impl FnMut(Vec<MetadataCell>) -> std::ops::ControlFlow<()>,
) {
    let mut buf: Vec<MetadataCell> = Vec::new();
    for path in paths {
        // Native tags (CPE-1175) read the path's native OS metadata directly — not its byte content — so
        // this skips the header read entirely for that column: no wasted 1 MiB read per row just to look
        // up an xattr/ADS. `read_native_tags` already degrades every failure mode (missing metadata, an
        // unsupported filesystem, an unreadable path) to an empty `NativeTags`, so this can never panic or
        // fail the batch — same skip-on-error guarantee as every other column below.
        if matches!(col, MetaColumn::NativeTags) {
            let cell = native_tags_cell(&read_native_tags(Path::new(path)));
            let display = cell.display(EMPTY_PLACEHOLDER);
            buf.push(MetadataCell { path: path.clone(), cell, display });
            if buf.len() >= batch && flush(std::mem::take(&mut buf)).is_break() {
                return;
            }
            continue;
        }

        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        // Skip-on-error: an unreadable file's bytes default to empty (+ not truncated), which every
        // extractor already turns into `CellValue::Empty` (never a panic — see `column_extract`'s fixtures).
        let (bytes, truncated) = read_header(Path::new(path)).unwrap_or_default();
        let mut cell = extract_column(&ext, &bytes, col);
        // CPE-1145 review fix: unlike the video/image extractors (which bounds-check structurally and bail to
        // Empty on truncation), `DocPages` is a linear `/Type /Page` substring scan with no end-of-structure
        // check — a page object living past the header cap would be silently UNDERCOUNTED (e.g. a 50-page
        // scanned PDF reported as "5"). A count from a truncated read can be *wrong*, not just missing, so
        // degrade it to Empty: honour the "never wrong, only empty" contract every other column keeps.
        if truncated && matches!(col, MetaColumn::DocPages) && matches!(cell, CellValue::Int(_)) {
            cell = CellValue::Empty;
        }
        let display = cell.display(EMPTY_PLACEHOLDER);
        buf.push(MetadataCell { path: path.clone(), cell, display });
        if buf.len() >= batch && flush(std::mem::take(&mut buf)).is_break() {
            return;
        }
    }
    if !buf.is_empty() {
        let _ = flush(buf);
    }
}

/// Collect-to-vec variant of [`stream_column_cells`] (tests + non-streaming callers): returns every cell
/// directly instead of streaming batches. Shares the same walk, so results always match the streaming
/// command.
pub fn column_cells(paths: &[String], col: MetaColumn) -> Vec<MetadataCell> {
    let mut out = Vec::new();
    stream_column_cells(paths, col, COLUMN_CELLS_BATCH, |batch| {
        out.extend(batch);
        std::ops::ControlFlow::Continue(())
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_column::AudioColumn;
    use crate::metadata_column::compare;
    use std::cmp::Ordering;

    /// A minimal ID3v2.3 tag from `(4-char id, latin1 text)` frames — mirrors `column_extract`'s own
    /// fixture builder (kept local so this module's tests don't reach into `column_extract`'s private
    /// test helpers).
    fn id3(frames: &[(&str, &str)]) -> Vec<u8> {
        fn syncsafe4(mut v: u32) -> [u8; 4] {
            let mut o = [0u8; 4];
            for i in (0..4).rev() {
                o[i] = (v & 0x7F) as u8;
                v >>= 7;
            }
            o
        }
        let mut body = Vec::new();
        for (id, text) in frames {
            let mut fb = vec![0u8];
            fb.extend_from_slice(text.as_bytes());
            body.extend_from_slice(id.as_bytes());
            body.extend_from_slice(&(fb.len() as u32).to_be_bytes());
            body.extend_from_slice(&[0, 0]);
            body.extend_from_slice(&fb);
        }
        let mut t = Vec::new();
        t.extend_from_slice(b"ID3");
        t.extend_from_slice(&[3, 0, 0]);
        t.extend_from_slice(&syncsafe4(body.len() as u32));
        t.extend_from_slice(&body);
        t
    }

    /// A real PNG's worth of bytes for a given `(w, h)`, via the `image` crate (same fixture pattern
    /// `image_column`'s own tests use).
    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::new(w, h);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
        buf
    }

    /// A minimal synthetic PDF: header + `page_count` `/Type /Page` objects (mirrors `column_extract`'s
    /// own fixture).
    fn pdf(page_count: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n");
        for n in 0..page_count {
            out.extend_from_slice(
                format!("{} 0 obj\n<< /Type /Page /Parent 1 0 R >>\nendobj\n", n + 1).as_bytes(),
            );
        }
        out
    }

    /// A scratch dir under the OS temp dir, unique per test run, cleaned up by the caller.
    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-colcells-{tag}"))
    }

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> String {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn column_cells_reads_real_files_across_families_and_sorts_type_aware() {
        let dir = scratch("mixed");

        let mp3_path = write_file(&dir, "song.mp3", &id3(&[("TRCK", "9"), ("TIT2", "Nine")]));
        let mp3_path_ten = write_file(&dir, "song10.mp3", &id3(&[("TRCK", "10"), ("TIT2", "Ten")]));
        let png_path = write_file(&dir, "photo.png", &png(120, 80));
        let pdf_path = write_file(&dir, "doc.pdf", &pdf(3));
        let txt_path = write_file(&dir, "notes.txt", b"not audio, not readable as a tag");
        let missing_path = dir.join("does-not-exist.mp3").to_string_lossy().into_owned();

        // --- Audio::Track column: numeric, sorts 9 before 10, and a non-audio/missing file is Empty. ---
        let paths = vec![
            mp3_path_ten.clone(),
            mp3_path.clone(),
            txt_path.clone(),
            missing_path.clone(),
        ];
        let cells = column_cells(&paths, MetaColumn::Audio(AudioColumn::Track));
        assert_eq!(cells.len(), 4);
        let by_path = |p: &str| cells.iter().find(|c| c.path == p).unwrap();
        assert_eq!(by_path(&mp3_path).cell, CellValue::Int(9));
        assert_eq!(by_path(&mp3_path).display, "9");
        assert_eq!(by_path(&mp3_path_ten).cell, CellValue::Int(10));
        assert_eq!(by_path(&txt_path).cell, CellValue::Empty);
        assert_eq!(by_path(&txt_path).display, "—");
        assert_eq!(by_path(&missing_path).cell, CellValue::Empty); // skip-on-error, never panics/fails

        // Numeric sort via CellValue::compare: 9 must sort before 10 (the classic lexical-sort bug).
        assert_eq!(
            compare(&by_path(&mp3_path).cell, &by_path(&mp3_path_ten).cell, true),
            Ordering::Less
        );

        // --- ImageDimensions column: a real PNG header yields Dimensions; a PDF under it is Empty. ---
        let dim_cells = column_cells(&[png_path.clone(), pdf_path.clone()], MetaColumn::ImageDimensions);
        let dim_by_path = |p: &str| dim_cells.iter().find(|c| c.path == p).unwrap();
        assert_eq!(dim_by_path(&png_path).cell, CellValue::Dimensions { w: 120, h: 80 });
        assert_eq!(dim_by_path(&png_path).display, "120 \u{00d7} 80");
        assert_eq!(dim_by_path(&pdf_path).cell, CellValue::Empty);

        // --- DocPages column: a real PDF header yields an Int page count. ---
        let page_cells = column_cells(std::slice::from_ref(&pdf_path), MetaColumn::DocPages);
        assert_eq!(page_cells[0].cell, CellValue::Int(3));
        assert_eq!(page_cells[0].display, "3");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// CPE-1145 review fix: `DocPages` counts `/Type /Page` markers by linear scan, so a PDF whose page
    /// objects extend PAST the 1 MiB header cap would be silently UNDERCOUNTED (a wrong, non-empty value).
    /// A count from a truncated read must degrade to `Empty` — never report a wrong page number.
    #[test]
    fn doc_pages_on_a_truncated_read_yields_empty_not_an_undercount() {
        let dir = scratch("pdf-truncated");

        // Two page objects near the start, then >1 MiB of filler, then three more page objects — so the
        // real page count is 5 but only the first 2 markers live within the capped header.
        let mut big = Vec::new();
        big.extend_from_slice(b"%PDF-1.4\n");
        for n in 0..2 {
            big.extend_from_slice(
                format!("{} 0 obj\n<< /Type /Page >>\nendobj\n", n + 1).as_bytes(),
            );
        }
        big.extend(std::iter::repeat_n(b'%', (HEADER_CAP as usize) + 4096)); // push past the cap
        for n in 2..5 {
            big.extend_from_slice(
                format!("{} 0 obj\n<< /Type /Page >>\nendobj\n", n + 1).as_bytes(),
            );
        }
        assert!(big.len() as u64 > HEADER_CAP, "fixture must exceed the header cap");
        let big_pdf = write_file(&dir, "scanned.pdf", &big);

        // Undercount (2, from the visible markers) would be WRONG → the fix returns Empty instead.
        let cells = column_cells(std::slice::from_ref(&big_pdf), MetaColumn::DocPages);
        assert_eq!(
            cells[0].cell,
            CellValue::Empty,
            "a truncated-read DocPages must be Empty, never a wrong undercount"
        );
        assert_eq!(cells[0].display, "—");

        // Control: a small in-cap PDF still reports its real page count (the fix only suppresses truncated reads).
        let small_pdf = write_file(&dir, "small.pdf", &pdf(4));
        let small = column_cells(std::slice::from_ref(&small_pdf), MetaColumn::DocPages);
        assert_eq!(small[0].cell, CellValue::Int(4));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stream_column_cells_batches_match_the_collected_vec() {
        let dir = scratch("stream");
        let mut paths = Vec::new();
        for i in 0..5 {
            paths.push(write_file(&dir, &format!("t{i}.mp3"), &id3(&[("TRCK", &(i + 1).to_string())])));
        }

        let collected = column_cells(&paths, MetaColumn::Audio(AudioColumn::Track));

        let mut streamed = Vec::new();
        let mut flushes = 0usize;
        stream_column_cells(&paths, MetaColumn::Audio(AudioColumn::Track), 2, |batch| {
            flushes += 1;
            assert!(batch.len() <= 2);
            streamed.extend(batch);
            std::ops::ControlFlow::Continue(())
        });

        assert!(flushes >= 3); // 5 items at batch size 2 → 3 flushes (2, 2, 1)
        assert_eq!(streamed.len(), collected.len());
        for (a, b) in streamed.iter().zip(collected.iter()) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.cell, b.cell);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stream_column_cells_honours_early_break() {
        let dir = scratch("break");
        let mut paths = Vec::new();
        for i in 0..10 {
            paths.push(write_file(&dir, &format!("t{i}.mp3"), &id3(&[("TRCK", "1")])));
        }

        let mut seen = 0usize;
        stream_column_cells(&paths, MetaColumn::Audio(AudioColumn::Track), 2, |batch| {
            seen += batch.len();
            std::ops::ControlFlow::Break(())
        });
        assert_eq!(seen, 2); // stopped after the first batch

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn available_columns_expose_id_label_and_extensions() {
        let cols = available_columns();
        assert_eq!(cols.len(), 37);
        let track = cols.iter().find(|c| c.id == "audio.track").expect("audio.track present");
        assert_eq!(track.column, MetaColumn::Audio(AudioColumn::Track));
        assert!(track.extensions.contains(&"mp3".to_string()));
        assert!(track.label.starts_with("Audio:"));

        // An applies-to-all detector column (CPE-1166) is exposed with an EMPTY extension list — the
        // sentinel the picker reads as "applies to every file" (never greyed out).
        let true_type = cols.iter().find(|c| c.id == "detect.true_type").expect("detect.true_type present");
        assert_eq!(true_type.column, MetaColumn::TrueType);
        assert!(true_type.extensions.is_empty(), "an applies-to-all column carries no extensions");
        assert_eq!(true_type.label, "True Type");

        // The native-tags column (CPE-1175) appears in the picker's list, applies-to-all like the
        // detectors above.
        let native_tags = cols.iter().find(|c| c.id == "native.tags").expect("native.tags present");
        assert_eq!(native_tags.column, MetaColumn::NativeTags);
        assert!(native_tags.extensions.is_empty(), "native tags apply to every file, not one extension");
        assert_eq!(native_tags.label, "Native Tags");
    }

    // --- CPE-1175: the native-tags column reads real per-path native metadata ---

    #[test]
    fn native_tags_column_reads_pushed_tags_and_is_blank_on_unsupported_or_absent() {
        use crate::native_bridge::push;
        use crate::tags::{tag_store_set, TagStore};

        let dir = scratch("native-tags");
        let tagged = write_file(&dir, "tagged.txt", b"contents");
        let untagged = write_file(&dir, "untagged.txt", b"contents");
        let missing = dir.join("does-not-exist.txt").to_string_lossy().into_owned();

        // Skip gracefully if this filesystem can't store native metadata at all (e.g. old-kernel tmpfs) —
        // a valid environment, not a code failure; `native_bridge`/`native_meta` cover that path in their
        // own tests.
        if !crate::native_meta::is_supported(Path::new(&tagged)) {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }

        // Push native tags onto one file only, via the store→native path the ticket says to reuse.
        let mut store = TagStore::new();
        tag_store_set(&mut store, &tagged, vec!["report".into(), "q3".into()], "red".into());
        push(&store, Path::new(&tagged)).unwrap();

        let cells = column_cells(
            &[tagged.clone(), untagged.clone(), missing.clone()],
            MetaColumn::NativeTags,
        );
        assert_eq!(cells.len(), 3);
        let by_path = |p: &str| cells.iter().find(|c| c.path == p).unwrap();

        // A file with native tags → comma-joined, normalized (sorted) order.
        assert_eq!(by_path(&tagged).cell, CellValue::Text("q3, report".into()));
        assert_eq!(by_path(&tagged).display, "q3, report");

        // A file with no native metadata at all → blank, never an error.
        assert_eq!(by_path(&untagged).cell, CellValue::Empty);
        assert_eq!(by_path(&untagged).display, "—");

        // A missing path → blank too — the skip-on-error guardrail, never a panic or a failed batch.
        assert_eq!(by_path(&missing).cell, CellValue::Empty);
        assert_eq!(by_path(&missing).display, "—");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_paths_yield_no_cells_and_no_flush() {
        let mut flushes = 0usize;
        stream_column_cells(&[], MetaColumn::DocPages, 64, |_| {
            flushes += 1;
            std::ops::ControlFlow::Continue(())
        });
        assert_eq!(flushes, 0);
        assert!(column_cells(&[], MetaColumn::DocPages).is_empty());
    }
}
