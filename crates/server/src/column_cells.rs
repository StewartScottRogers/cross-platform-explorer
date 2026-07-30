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

use crate::column_extract::{extract_column, MetaColumn};
use crate::metadata_column::CellValue;

/// One pickable metadata column for the picker UI: a stable string id (for persistence in
/// [`crate::column_config`]), a friendly label, the typed [`MetaColumn`] to pass back into
/// [`column_cells`]/the streaming command, and the lowercase extensions it applies to (so the picker can
/// grey out a non-applicable row).
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

/// Read a capped header of `path` (at most [`HEADER_CAP`] bytes). Any I/O failure (missing file, no
/// permission, a directory, …) surfaces as `Err`; [`stream_column_cells`] below turns that into an empty
/// cell rather than aborting the batch.
fn read_header(path: &Path) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(HEADER_CAP).read_to_end(&mut bytes)?;
    Ok(bytes)
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
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        // Skip-on-error: an unreadable file's bytes default to empty, which every extractor already
        // turns into `CellValue::Empty` (never a panic — see `column_extract`'s own fixture tests).
        let bytes = read_header(Path::new(path)).unwrap_or_default();
        let cell = extract_column(&ext, &bytes, col);
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
    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, AtomicOrdering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-colcells-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
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
        assert_eq!(cols.len(), 32);
        let track = cols.iter().find(|c| c.id == "audio.track").expect("audio.track present");
        assert_eq!(track.column, MetaColumn::Audio(AudioColumn::Track));
        assert!(track.extensions.contains(&"mp3".to_string()));
        assert!(track.label.starts_with("Audio:"));
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
