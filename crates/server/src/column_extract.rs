//! Metadata-column dispatcher (CPE-975, epic CPE-707): the single entry point that turns a file's bytes +
//! extension + a requested column into a typed [`CellValue`], routing to the right per-family extractor.
//!
//! This unifies the pieces the details-view column system needs: the audio read codecs
//! ([`crate::media_meta_read`]: ID3 / FLAC / OGG) feeding the audio column typing
//! ([`crate::media_column`]), and the image header reader ([`crate::image_column`]). A caller (the column
//! UI, an MCP tool, a command) picks a [`MetaColumn`] and hands over the file's leading bytes; this decides
//! the codec by extension and returns the cell — or [`CellValue::Empty`] when the file kind doesn't match
//! the column (so it sorts last). Pure: no filesystem, the adapter reads the bytes.

use serde::{Deserialize, Serialize};

use crate::doc_column::doc_pages_cell;
use crate::doc_info_column::{doc_info_cell, DocInfoColumn};
use crate::file_type::{detect_type, mismatch};
use crate::image_column::image_dimensions_cell;
use crate::media_column::{audio_cell, AudioColumn};
use crate::media_meta_edit::MetaField;
use crate::media_meta_read::{read_flac, read_id3v2, read_ogg, read_pdf, read_wav};
use crate::metadata_column::CellValue;
use crate::native_tags::NativeTags;
use crate::text_encoding::{detect_encoding, detect_line_endings, EncodingGuess, LineEnding};
use crate::video_column::video_cell;
use crate::video_meta_read::read_mp4;
use crate::video_tag_column::{video_tag_cell, VideoTagColumn};

/// Audio file extensions the audio-tag extractors read (ID3v2/FLAC/OGG-Vorbis/RIFF-INFO).
const AUDIO_EXTS: &[&str] = &["mp3", "flac", "ogg", "oga", "wav"];
/// Image extensions the pixel-dimensions header reader attempts.
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif"];
/// Document extensions the page-count / doc-info readers attempt (PDF only, v1).
const DOC_EXTS: &[&str] = &["pdf"];
/// ISO-BMFF video extensions the duration / tag readers attempt.
const VIDEO_EXTS: &[&str] = &["mp4", "mov", "m4v"];

/// A metadata column the details view can add, spanning media families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum MetaColumn {
    /// A typed audio-tag column (Title/Artist/Track/Year/…), read from ID3/FLAC/OGG by extension.
    Audio(AudioColumn),
    /// The image's pixel dimensions (`w × h`, sorted by area).
    ImageDimensions,
    /// A document's page count (PDF, v1), sorted numerically.
    DocPages,
    /// A typed document-info column (Title/Author/Subject/…), read from a PDF's `/Info` dictionary
    /// (CPE-1039).
    DocInfo(DocInfoColumn),
    /// The video's duration in seconds, read from the ISO-BMFF `moov/mvhd` box (CPE-1028).
    VideoDuration,
    /// A typed video-tag column (Title/Artist/Album/Year/…), read from an MP4/MOV's
    /// `moov/udta/meta/ilst` iTunes-style tags (CPE-1040).
    VideoTag(VideoTagColumn),
    /// The file's true type detected from its leading magic bytes (CPE-1001), e.g. `"PNG image"`; empty
    /// when no signature is recognised. Unlike the media-family columns above this **applies to every
    /// file** (the empty-[`extensions`](MetaColumn::extensions) sentinel — see [`MetaColumn::applies_to_all`]).
    TrueType,
    /// A flag when the detected true type disagrees with the file's extension (CPE-1001) — a disguised
    /// file, e.g. a `.jpg` that is really a Windows executable (`"mismatch: exe"`). Empty when they agree,
    /// the type is unknown, or there is no extension. Applies to every file.
    TypeMismatch,
    /// The file's guessed text encoding (CPE-1003), e.g. `"UTF-8"`, `"Latin-1 / 8-bit (guessed)"`,
    /// `"Binary"`; empty for a zero-byte file. Applies to every file.
    TextEncoding,
    /// The file's dominant line-ending convention (CPE-1003): `"LF (Unix)"`, `"CRLF (Windows)"`,
    /// `"CR (classic Mac)"`, or `"Mixed"`. Empty for binary/empty files and text with no line breaks.
    /// Applies to every file.
    LineEndings,
    /// The path's **native OS tags** (CPE-1175, epic CPE-717) — Finder tags / NTFS ADS / xattr —
    /// comma-joined, read lazily per row via [`crate::native_bridge::read_native_tags`] (never the
    /// internal tag store, never the hot `list_dir` path). Empty when the path has no native metadata,
    /// the filesystem can't store it (FAT, no xattr support), or it isn't readable. Applies to every file
    /// (and directory — native tags aren't extension-scoped), so it uses the same applies-to-all
    /// sentinel as the CPE-1166 detectors.
    NativeTags,
}

impl MetaColumn {
    /// Every pickable column, in a stable display order (family by family) — what
    /// [`crate::column_cells::available_columns`] enumerates for the picker UI (CPE-1145, epic CPE-707).
    pub fn all() -> Vec<MetaColumn> {
        let mut out: Vec<MetaColumn> = Vec::with_capacity(
            AudioColumn::ALL.len() + 3 + DocInfoColumn::ALL.len() + VideoTagColumn::ALL.len(),
        );
        out.extend(AudioColumn::ALL.into_iter().map(MetaColumn::Audio));
        out.push(MetaColumn::ImageDimensions);
        out.push(MetaColumn::DocPages);
        out.extend(DocInfoColumn::ALL.into_iter().map(MetaColumn::DocInfo));
        out.push(MetaColumn::VideoDuration);
        out.extend(VideoTagColumn::ALL.into_iter().map(MetaColumn::VideoTag));
        // Magic-byte detectors (CPE-1166) — apply to every file, so they follow the family columns.
        out.push(MetaColumn::TrueType);
        out.push(MetaColumn::TypeMismatch);
        out.push(MetaColumn::TextEncoding);
        out.push(MetaColumn::LineEndings);
        // Native OS tags (CPE-1175) — also applies-to-all, opt-in, lazy per-row read.
        out.push(MetaColumn::NativeTags);
        out
    }

    /// A stable string id for this column (family-prefixed snake_case, e.g. `"audio.track"`,
    /// `"image.dimensions"`, `"doc.info.title"`), for persisting the picker's chosen columns in
    /// [`crate::column_config`] — that store keeps string ids so it stays decoupled from this enum.
    pub fn id(&self) -> String {
        match self {
            MetaColumn::Audio(c) => format!("audio.{}", c.id_token()),
            MetaColumn::ImageDimensions => "image.dimensions".to_string(),
            MetaColumn::DocPages => "doc.pages".to_string(),
            MetaColumn::DocInfo(c) => format!("doc.info.{}", c.id_token()),
            MetaColumn::VideoDuration => "video.duration".to_string(),
            MetaColumn::VideoTag(c) => format!("video.tag.{}", c.id_token()),
            MetaColumn::TrueType => "detect.true_type".to_string(),
            MetaColumn::TypeMismatch => "detect.type_mismatch".to_string(),
            MetaColumn::TextEncoding => "detect.text_encoding".to_string(),
            MetaColumn::LineEndings => "detect.line_endings".to_string(),
            MetaColumn::NativeTags => "native.tags".to_string(),
        }
    }

    /// The friendly display label for the column picker — family-prefixed so a name that recurs across
    /// families (e.g. "Year" in both Audio and VideoTag) is unambiguous in a flat list.
    pub fn label(&self) -> String {
        match self {
            MetaColumn::Audio(c) => format!("Audio: {}", c.label()),
            MetaColumn::ImageDimensions => "Image Dimensions".to_string(),
            MetaColumn::DocPages => "Page Count".to_string(),
            MetaColumn::DocInfo(c) => format!("Document: {}", c.label()),
            MetaColumn::VideoDuration => "Video Duration".to_string(),
            MetaColumn::VideoTag(c) => format!("Video: {}", c.label()),
            MetaColumn::TrueType => "True Type".to_string(),
            MetaColumn::TypeMismatch => "Type Mismatch".to_string(),
            MetaColumn::TextEncoding => "Text Encoding".to_string(),
            MetaColumn::LineEndings => "Line Endings".to_string(),
            MetaColumn::NativeTags => "Native Tags".to_string(),
        }
    }

    /// The lowercase extensions this column applies to, so the picker can grey out a non-applicable row
    /// (mirrors the gating `extract_column` already does internally).
    ///
    /// **"Applies to all files" sentinel (CPE-1166):** an **empty** slice means the column applies to
    /// *every* file, not a specific media family — the magic-byte detectors (true type / mismatch /
    /// encoding / line endings) are file-agnostic, and so is the native-tags column (CPE-1175): OS-level
    /// tags aren't scoped to a file kind. The picker must treat empty-extensions as applies-to-all and
    /// never grey such a column out. See [`MetaColumn::applies_to_all`].
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            MetaColumn::Audio(_) => AUDIO_EXTS,
            MetaColumn::ImageDimensions => IMAGE_EXTS,
            MetaColumn::DocPages | MetaColumn::DocInfo(_) => DOC_EXTS,
            MetaColumn::VideoDuration | MetaColumn::VideoTag(_) => VIDEO_EXTS,
            // Applies-to-all sentinel: the file-agnostic magic-byte detectors and the native-tags column
            // (CPE-1175) have no extension gate.
            MetaColumn::TrueType
            | MetaColumn::TypeMismatch
            | MetaColumn::TextEncoding
            | MetaColumn::LineEndings
            | MetaColumn::NativeTags => &[],
        }
    }

    /// Whether this column applies to **every** file rather than one media family — the "applies to all
    /// files" sentinel (CPE-1166): an empty [`extensions`](MetaColumn::extensions) list. The magic-byte
    /// detectors and the native-tags column (CPE-1175) are the only such columns; the picker must never
    /// grey them out on an unrecognised extension.
    pub fn applies_to_all(&self) -> bool {
        self.extensions().is_empty()
    }
}

/// Read a file's audio tags, choosing the codec by extension: `mp3` → ID3v2, `flac` → FLAC/Vorbis,
/// `ogg`/`oga` → OGG/Vorbis, `wav` → RIFF/INFO. A non-audio (or unrecognised) extension yields no fields.
pub fn read_audio_tags(ext: &str, bytes: &[u8]) -> Vec<MetaField> {
    match ext.to_ascii_lowercase().as_str() {
        "mp3" => read_id3v2(bytes),
        "flac" => read_flac(bytes),
        "ogg" | "oga" => read_ogg(bytes),
        "wav" => read_wav(bytes),
        _ => Vec::new(),
    }
}

/// Whether `ext` is an image kind the dimensions reader should attempt (avoids decoding non-images).
fn is_image_ext(ext: &str) -> bool {
    IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

/// Whether `ext` is a document kind the page-count reader should attempt (PDF only, v1).
fn is_doc_ext(ext: &str) -> bool {
    DOC_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

/// Whether `ext` is an ISO-BMFF video kind the duration reader should attempt (avoids walking the box
/// tree of unrelated files).
fn is_video_ext(ext: &str) -> bool {
    VIDEO_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

/// The typed [`CellValue`] for `col` from a file's `ext` + leading `bytes`, dispatched to the family
/// extractor. A file whose kind doesn't match the column (e.g. an image path under an audio column, or a
/// text file under a Dimensions column) yields [`CellValue::Empty`], which sorts last.
pub fn extract_column(ext: &str, bytes: &[u8], col: MetaColumn) -> CellValue {
    match col {
        MetaColumn::Audio(audio) => audio_cell(&read_audio_tags(ext, bytes), audio),
        MetaColumn::ImageDimensions => {
            if is_image_ext(ext) {
                image_dimensions_cell(bytes)
            } else {
                CellValue::Empty
            }
        }
        MetaColumn::DocPages => {
            if is_doc_ext(ext) {
                doc_pages_cell(bytes)
            } else {
                CellValue::Empty
            }
        }
        MetaColumn::DocInfo(col) => {
            if is_doc_ext(ext) {
                doc_info_cell(&read_pdf(bytes), col)
            } else {
                CellValue::Empty
            }
        }
        MetaColumn::VideoDuration => {
            if is_video_ext(ext) {
                video_cell(bytes)
            } else {
                CellValue::Empty
            }
        }
        MetaColumn::VideoTag(col) => {
            if is_video_ext(ext) {
                video_tag_cell(&read_mp4(bytes), col)
            } else {
                CellValue::Empty
            }
        }
        // Magic-byte detectors (CPE-1166) — file-agnostic, so no extension gate; they read only the
        // leading bytes the caller supplies (the detectors cap their own scans internally).
        MetaColumn::TrueType => match detect_type(bytes) {
            Some(ft) => CellValue::Text(ft.label().to_string()),
            None => CellValue::Empty,
        },
        MetaColumn::TypeMismatch => match mismatch(bytes, ext) {
            // Compact flag naming what the bytes really are, via the detected type's canonical extension
            // (e.g. a PE disguised as `.jpg` → "mismatch: exe").
            Some(m) => CellValue::Text(format!(
                "mismatch: {}",
                m.detected.extensions().first().copied().unwrap_or("")
            )),
            None => CellValue::Empty,
        },
        MetaColumn::TextEncoding => match detect_encoding(bytes) {
            // A zero-byte file has no meaningful encoding → no value (sorts last), not the "Empty file"
            // label — keeps the column's blanks consistent with every other column's `Empty`.
            EncodingGuess::Empty => CellValue::Empty,
            enc => CellValue::Text(enc.label().to_string()),
        },
        MetaColumn::LineEndings => line_endings_cell(bytes),
        // Native tags (CPE-1175) can't be computed from header bytes at all — unlike every other column
        // here, the value lives in the path's native OS metadata (Finder tags / NTFS ADS / xattr), not
        // its content. The real per-row read happens in `column_cells::stream_column_cells`, which has
        // the actual filesystem path and calls `native_tags_cell` (below) against
        // `native_bridge::read_native_tags(path)` directly — skipping this bytes-only dispatcher (and the
        // header read) entirely for this column. This arm exists only so `extract_column` stays
        // exhaustive over `MetaColumn` and never panics if ever called with it directly.
        MetaColumn::NativeTags => CellValue::Empty,
    }
}

/// The [`CellValue`] for a path's native tags (CPE-1175): its tag names comma-joined in their normalized
/// (sorted, de-duped) order, or [`CellValue::Empty`] when there are none — no native metadata, an
/// unsupported filesystem, or an unreadable path all collapse to the same empty cell here (the read side,
/// [`crate::native_bridge::read_native_tags`], already degrades every failure mode to
/// [`NativeTags::default`]). Pure: takes the already-decoded [`NativeTags`], no filesystem access.
pub fn native_tags_cell(native: &NativeTags) -> CellValue {
    if native.tags.is_empty() {
        CellValue::Empty
    } else {
        CellValue::Text(native.tags.join(", "))
    }
}

/// The line-ending cell for a file's leading `bytes` (CPE-1166): decode as text and report the dominant
/// convention, mirroring [`crate::inspect`]'s Properties-panel logic. Binary/empty files — and text with
/// no line breaks — yield [`CellValue::Empty`]. Computed over the (capped) header bytes only, so it is a
/// sample of the file's convention, not an exhaustive scan (an accepted per-row trade-off).
fn line_endings_cell(bytes: &[u8]) -> CellValue {
    match detect_encoding(bytes) {
        EncodingGuess::Binary | EncodingGuess::Empty => CellValue::Empty,
        _ => {
            let report = detect_line_endings(&String::from_utf8_lossy(bytes));
            if report.crlf == 0 && report.lf == 0 && report.cr == 0 {
                return CellValue::Empty;
            }
            if report.mixed {
                return CellValue::Text("Mixed".to_string());
            }
            match report.dominant {
                LineEnding::Crlf => CellValue::Text("CRLF (Windows)".to_string()),
                LineEnding::Lf => CellValue::Text("LF (Unix)".to_string()),
                LineEnding::Cr => CellValue::Text("CR (classic Mac)".to_string()),
                LineEnding::None | LineEnding::Mixed => CellValue::Empty,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // --- compact fixture builders (routing tests, not re-verifying the codecs) ---

    fn syncsafe4(mut v: u32) -> [u8; 4] {
        let mut o = [0u8; 4];
        for i in (0..4).rev() {
            o[i] = (v & 0x7F) as u8;
            v >>= 7;
        }
        o
    }

    /// A minimal ID3v2.3 tag from `(4-char id, latin1 text)` frames.
    fn id3(frames: &[(&str, &str)]) -> Vec<u8> {
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

    fn vorbis_block(comments: &[&str]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for c in comments {
            b.extend_from_slice(&(c.len() as u32).to_le_bytes());
            b.extend_from_slice(c.as_bytes());
        }
        b
    }

    fn flac(comments: &[&str]) -> Vec<u8> {
        let block = vorbis_block(comments);
        let mut f = Vec::new();
        f.extend_from_slice(b"fLaC");
        f.push(0x84); // last block, type 4
        f.extend_from_slice(&(block.len() as u32).to_be_bytes()[1..]);
        f.extend_from_slice(&block);
        f
    }

    /// A minimal single-page OGG stream (CPE-1133: `read_ogg` now walks real page framing, so this must
    /// be a well-formed page — one segment whose lace value is the packet's exact length — rather than a
    /// raw byte scan target).
    fn ogg(comments: &[&str]) -> Vec<u8> {
        let mut packet = b"\x03vorbis".to_vec();
        packet.extend_from_slice(&vorbis_block(comments));
        assert!(packet.len() < 255, "fixture must fit in a single lace segment");
        let mut o = Vec::new();
        o.extend_from_slice(b"OggS");
        o.extend_from_slice(&[0u8; 22]); // version + header_type + granule + serial + seqno + checksum (stubbed)
        o.push(1); // page_segments: one segment
        o.push(packet.len() as u8); // lace value < 255 terminates the packet at exactly its length
        o.extend_from_slice(&packet);
        o
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::new(w, h);
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
        buf
    }

    /// A minimal synthetic MP4: `moov/mvhd` (version 0) with `timescale`/`duration` chosen so the
    /// duration is exactly `seconds` (routing test only — the box-tree parser itself is covered in
    /// `video_column`'s own tests).
    fn mp4(seconds: u32) -> Vec<u8> {
        let mut mvhd_content = vec![0u8, 0, 0, 0]; // version 0 + flags
        mvhd_content.extend_from_slice(&0u32.to_be_bytes()); // creation_time
        mvhd_content.extend_from_slice(&0u32.to_be_bytes()); // modification_time
        mvhd_content.extend_from_slice(&1u32.to_be_bytes()); // timescale = 1
        mvhd_content.extend_from_slice(&seconds.to_be_bytes()); // duration in timescale units

        let mut mvhd = Vec::new();
        mvhd.extend_from_slice(&((8 + mvhd_content.len()) as u32).to_be_bytes());
        mvhd.extend_from_slice(b"mvhd");
        mvhd.extend_from_slice(&mvhd_content);

        let mut moov = Vec::new();
        moov.extend_from_slice(&((8 + mvhd.len()) as u32).to_be_bytes());
        moov.extend_from_slice(b"moov");
        moov.extend_from_slice(&mvhd);
        moov
    }

    #[test]
    fn routes_audio_by_extension_to_the_right_codec() {
        // mp3 → ID3
        assert_eq!(
            extract_column("mp3", &id3(&[("TRCK", "5/10")]), MetaColumn::Audio(AudioColumn::Track)),
            CellValue::Int(5)
        );
        // FLAC → Vorbis
        assert_eq!(
            extract_column("flac", &flac(&["ARTIST=Boards of Canada"]), MetaColumn::Audio(AudioColumn::Artist)),
            CellValue::Text("Boards of Canada".into())
        );
        // OGG → Vorbis
        assert_eq!(
            extract_column("ogg", &ogg(&["TITLE=Roygbiv"]), MetaColumn::Audio(AudioColumn::Title)),
            CellValue::Text("Roygbiv".into())
        );
        // Case-insensitive extension.
        assert_eq!(
            extract_column("FLAC", &flac(&["ALBUM=Geogaddi"]), MetaColumn::Audio(AudioColumn::Album)),
            CellValue::Text("Geogaddi".into())
        );
    }

    #[test]
    fn image_dimensions_route_and_gate_by_extension() {
        assert_eq!(
            extract_column("png", &png(120, 80), MetaColumn::ImageDimensions),
            CellValue::Dimensions { w: 120, h: 80 }
        );
        // A non-image extension is not even attempted → Empty (even if bytes happened to be an image).
        assert_eq!(extract_column("txt", &png(10, 10), MetaColumn::ImageDimensions), CellValue::Empty);
    }

    /// A minimal synthetic PDF: header + `page_count` `/Type /Page` objects.
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

    #[test]
    fn doc_pages_route_and_gate_by_extension() {
        assert_eq!(extract_column("pdf", &pdf(2), MetaColumn::DocPages), CellValue::Int(2));
        // A non-doc extension is not even attempted → Empty (even if bytes happened to be a PDF).
        assert_eq!(extract_column("txt", &pdf(2), MetaColumn::DocPages), CellValue::Empty);
    }

    /// A minimal synthetic PDF carrying an inline `/Info << … >>` dictionary (the `pdf()` helper above
    /// has no `/Info` at all, so DocInfo routing needs its own fixture).
    fn pdf_with_info(title: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n");
        out.extend_from_slice(format!("/Info << /Title ({title}) >>\n").as_bytes());
        out.extend_from_slice(b"%%EOF");
        out
    }

    #[test]
    fn doc_info_route_and_gate_by_extension() {
        assert_eq!(
            extract_column("pdf", &pdf_with_info("Vacation Photos"), MetaColumn::DocInfo(DocInfoColumn::Title)),
            CellValue::Text("Vacation Photos".into())
        );
        // A non-doc extension is not even attempted → Empty (even if bytes happened to be a PDF).
        assert_eq!(
            extract_column("txt", &pdf_with_info("Vacation Photos"), MetaColumn::DocInfo(DocInfoColumn::Title)),
            CellValue::Empty
        );
        // A PDF with no /Info at all → Empty.
        assert_eq!(extract_column("pdf", &pdf(1), MetaColumn::DocInfo(DocInfoColumn::Title)), CellValue::Empty);
    }

    #[test]
    fn video_duration_route_and_gate_by_extension() {
        assert_eq!(extract_column("mp4", &mp4(5), MetaColumn::VideoDuration), CellValue::Float(5.0));
        // A non-video extension is not even attempted → Empty (even if bytes happened to be a video).
        assert_eq!(extract_column("txt", &mp4(5), MetaColumn::VideoDuration), CellValue::Empty);
    }

    /// Wrap `content` in a box of the given 4-byte `type` (32-bit size — plenty for these fixtures).
    /// Mirrors `video_meta_read`'s own `make_box` test helper (this file builds a *tagged* MP4, unlike
    /// the `mp4()` helper above which only carries `moov/mvhd` for duration).
    fn make_box(box_type: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        let total = (8 + content.len()) as u32;
        b.extend_from_slice(&total.to_be_bytes());
        b.extend_from_slice(box_type);
        b.extend_from_slice(content);
        b
    }

    /// A `data` box wrapping UTF-8 `text` with the "well-known type = UTF-8 text" flag (1).
    fn data_box_text(text: &str) -> Vec<u8> {
        let mut content = Vec::new();
        content.extend_from_slice(&1u32.to_be_bytes()); // version(0) + flags(1) = UTF-8 text
        content.extend_from_slice(&0u32.to_be_bytes()); // reserved
        content.extend_from_slice(text.as_bytes());
        make_box(b"data", &content)
    }

    /// A minimal MP4 carrying `moov/udta/meta/ilst` tags: `©nam`(Title) and `©ART`(Artist), each wrapping a
    /// UTF-8 `data` atom (routing test only — the box-tree parser itself is covered in
    /// `video_meta_read`'s own tests).
    fn mp4_with_tags(title: &str, artist: &str) -> Vec<u8> {
        const NAM: [u8; 4] = [0xA9, b'n', b'a', b'm'];
        const ART: [u8; 4] = [0xA9, b'A', b'R', b'T'];

        let mut ilst_content = Vec::new();
        ilst_content.extend_from_slice(&make_box(&NAM, &data_box_text(title)));
        ilst_content.extend_from_slice(&make_box(&ART, &data_box_text(artist)));
        let ilst = make_box(b"ilst", &ilst_content);

        let mut meta_content = Vec::new();
        meta_content.extend_from_slice(&[0, 0, 0, 0]); // meta's version+flags prelude
        meta_content.extend_from_slice(&ilst);
        let meta = make_box(b"meta", &meta_content);

        let udta = make_box(b"udta", &meta);
        let moov = make_box(b"moov", &udta);

        let mut f = Vec::new();
        f.extend_from_slice(&make_box(b"ftyp", b"isom"));
        f.extend_from_slice(&moov);
        f
    }

    #[test]
    fn video_tag_route_and_gate_by_extension() {
        let file = mp4_with_tags("Big Buck Bunny", "Blender Foundation");
        assert_eq!(
            extract_column("mp4", &file, MetaColumn::VideoTag(VideoTagColumn::Title)),
            CellValue::Text("Big Buck Bunny".into())
        );
        assert_eq!(
            extract_column("mp4", &file, MetaColumn::VideoTag(VideoTagColumn::Artist)),
            CellValue::Text("Blender Foundation".into())
        );
        // A non-video extension is not even attempted → Empty (even if bytes happened to be a tagged MP4).
        assert_eq!(
            extract_column("txt", &file, MetaColumn::VideoTag(VideoTagColumn::Title)),
            CellValue::Empty
        );
        // An MP4 with no udta/tags at all → Empty.
        assert_eq!(
            extract_column("mp4", &mp4(5), MetaColumn::VideoTag(VideoTagColumn::Title)),
            CellValue::Empty
        );
    }

    #[test]
    fn mismatched_kind_yields_empty() {
        // An audio column on an image file → no audio codec matches → Empty.
        assert_eq!(
            extract_column("png", &png(4, 4), MetaColumn::Audio(AudioColumn::Artist)),
            CellValue::Empty
        );
        // Unknown extension → Empty.
        assert_eq!(extract_column("xyz", b"whatever", MetaColumn::Audio(AudioColumn::Title)), CellValue::Empty);
    }

    // --- CPE-1166: the applies-to-all magic-byte detectors ---

    fn pe_bytes() -> Vec<u8> {
        vec![0x4D, 0x5A, 0x90, 0x00] // "MZ" DOS stub → Windows PE (exe/dll)
    }

    #[test]
    fn true_type_detects_across_extensions_and_is_empty_for_unrecognised_bytes() {
        // Real PNG bytes under a `.txt` name still detect the true type — the column applies to all files,
        // not just image extensions.
        assert_eq!(
            extract_column("txt", &png(4, 4), MetaColumn::TrueType),
            CellValue::Text("PNG image".into())
        );
        // A PE, whatever the extension.
        assert_eq!(
            extract_column("jpg", &pe_bytes(), MetaColumn::TrueType),
            CellValue::Text("Windows executable/library".into())
        );
        // Plain text has no magic signature → no value.
        assert_eq!(extract_column("txt", b"just some plain text", MetaColumn::TrueType), CellValue::Empty);
    }

    #[test]
    fn type_mismatch_flags_a_disguised_file_and_is_empty_when_consistent() {
        // A PE renamed to `.jpg` → flagged with the detected type's canonical extension (CPE-1166 example).
        assert_eq!(
            extract_column("jpg", &pe_bytes(), MetaColumn::TypeMismatch),
            CellValue::Text("mismatch: exe".into())
        );
        // PNG bytes under a `.txt` name → mismatch, flagged as the real ".png".
        assert_eq!(
            extract_column("txt", &png(4, 4), MetaColumn::TypeMismatch),
            CellValue::Text("mismatch: png".into())
        );
        // PNG bytes correctly named `.png` → no mismatch.
        assert_eq!(extract_column("png", &png(4, 4), MetaColumn::TypeMismatch), CellValue::Empty);
        // Unknown bytes → nothing to contradict → Empty.
        assert_eq!(extract_column("txt", b"plain text", MetaColumn::TypeMismatch), CellValue::Empty);
        // A detected type with no extension → nothing to disagree with → Empty.
        assert_eq!(extract_column("", &pe_bytes(), MetaColumn::TypeMismatch), CellValue::Empty);
    }

    #[test]
    fn text_encoding_reports_utf8_latin1_binary_and_empty() {
        // UTF-8 ASCII.
        assert_eq!(
            extract_column("txt", b"plain ascii text", MetaColumn::TextEncoding),
            CellValue::Text("UTF-8".into())
        );
        // 0xE9 alone ('é' in Latin-1) is invalid UTF-8 but not binary-looking → Latin-1 guess.
        assert_eq!(
            extract_column("txt", &[0xE9], MetaColumn::TextEncoding),
            CellValue::Text("Latin-1 / 8-bit (guessed)".into())
        );
        // A PNG's bytes read as Binary (NUL bytes in the header).
        assert_eq!(
            extract_column("png", &png(4, 4), MetaColumn::TextEncoding),
            CellValue::Text("Binary".into())
        );
        // A zero-byte file → no value (not the "Empty file" label), so it sorts last like any blank cell.
        assert_eq!(extract_column("txt", b"", MetaColumn::TextEncoding), CellValue::Empty);
    }

    #[test]
    fn line_endings_report_lf_crlf_mixed_and_empty() {
        assert_eq!(
            extract_column("txt", b"a\nb\nc\n", MetaColumn::LineEndings),
            CellValue::Text("LF (Unix)".into())
        );
        assert_eq!(
            extract_column("txt", b"a\r\nb\r\n", MetaColumn::LineEndings),
            CellValue::Text("CRLF (Windows)".into())
        );
        // Both conventions present → Mixed.
        assert_eq!(
            extract_column("txt", b"a\r\nb\n", MetaColumn::LineEndings),
            CellValue::Text("Mixed".into())
        );
        // Text with no line breaks → no value.
        assert_eq!(extract_column("txt", b"one line, no break", MetaColumn::LineEndings), CellValue::Empty);
        // Binary → no value.
        assert_eq!(extract_column("png", &png(4, 4), MetaColumn::LineEndings), CellValue::Empty);
        // Empty file → no value.
        assert_eq!(extract_column("txt", b"", MetaColumn::LineEndings), CellValue::Empty);
    }

    // --- CPE-1175: the native-tags column ---

    #[test]
    fn extract_column_native_tags_is_always_empty_regardless_of_bytes() {
        // `extract_column` is pure-over-bytes and has no filesystem access, so it can never compute the
        // real native-tags value — the actual per-path read happens in `column_cells` via
        // `native_bridge::read_native_tags` + `native_tags_cell`. This just asserts the dispatcher arm
        // never panics and always yields Empty, whatever bytes/extension it's handed.
        assert_eq!(extract_column("txt", b"whatever", MetaColumn::NativeTags), CellValue::Empty);
        assert_eq!(extract_column("", b"", MetaColumn::NativeTags), CellValue::Empty);
    }

    #[test]
    fn native_tags_cell_comma_joins_tags_and_is_empty_when_none() {
        assert_eq!(
            native_tags_cell(&NativeTags::new(vec!["q3".into(), "report".into()], String::new())),
            CellValue::Text("q3, report".into())
        );
        // Normalization sorts + de-dupes, so the join order is stable regardless of input order.
        assert_eq!(
            native_tags_cell(&NativeTags::new(vec!["zebra".into(), "apple".into()], String::new())),
            CellValue::Text("apple, zebra".into())
        );
        // No tags (a native blob that's absent, unsupported, or carries only a label) → Empty.
        assert_eq!(native_tags_cell(&NativeTags::default()), CellValue::Empty);
        assert_eq!(
            native_tags_cell(&NativeTags::new(vec![], "red".into())),
            CellValue::Empty
        );
    }

    #[test]
    fn read_audio_tags_dispatches_and_is_empty_for_non_audio() {
        assert!(!read_audio_tags("mp3", &id3(&[("TIT2", "X")])).is_empty());
        assert!(read_audio_tags("png", &png(2, 2)).is_empty());
        assert!(read_audio_tags("", b"").is_empty());
    }

    #[test]
    fn all_columns_have_unique_ids_and_nonempty_labels() {
        let all = MetaColumn::all();
        // 12 audio + 1 dimensions + 1 pages + 8 doc-info + 1 duration + 9 video-tag + 4 detectors + 1 native tags.
        assert_eq!(all.len(), 37);

        let mut ids: Vec<String> = all.iter().map(MetaColumn::id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "every column must have a unique id");

        // Every column has a label. Extensions are NOT asserted non-empty any more: the applies-to-all
        // sentinel (CPE-1166) deliberately uses an empty list — see the dedicated sentinel test below.
        for col in &all {
            assert!(!col.label().is_empty());
        }
    }

    #[test]
    fn applies_to_all_sentinel_is_empty_extensions_for_untyped_columns() {
        // The "applies to all files" sentinel (CPE-1166): a column with EMPTY extensions applies to every
        // file and must never be greyed out by the extension gate. The magic-byte detectors plus the
        // native-tags column (CPE-1175) are the only such columns; every media-family column keeps a
        // non-empty extension list.
        for col in [
            MetaColumn::TrueType,
            MetaColumn::TypeMismatch,
            MetaColumn::TextEncoding,
            MetaColumn::LineEndings,
            MetaColumn::NativeTags,
        ] {
            assert!(col.extensions().is_empty(), "{} must apply to all files", col.id());
            assert!(col.applies_to_all(), "{} must report applies_to_all", col.id());
        }
        // A media-family column is still extension-scoped, never applies-to-all.
        assert!(!MetaColumn::ImageDimensions.extensions().is_empty());
        assert!(!MetaColumn::ImageDimensions.applies_to_all());
        assert!(!MetaColumn::Audio(AudioColumn::Title).applies_to_all());
    }

    #[test]
    fn column_ids_are_stable_family_prefixed_tokens() {
        assert_eq!(MetaColumn::Audio(AudioColumn::Track).id(), "audio.track");
        assert_eq!(MetaColumn::ImageDimensions.id(), "image.dimensions");
        assert_eq!(MetaColumn::DocPages.id(), "doc.pages");
        assert_eq!(MetaColumn::DocInfo(DocInfoColumn::Author).id(), "doc.info.author");
        assert_eq!(MetaColumn::VideoDuration.id(), "video.duration");
        assert_eq!(MetaColumn::VideoTag(VideoTagColumn::Year).id(), "video.tag.year");
        assert_eq!(MetaColumn::TrueType.id(), "detect.true_type");
        assert_eq!(MetaColumn::TypeMismatch.id(), "detect.type_mismatch");
        assert_eq!(MetaColumn::TextEncoding.id(), "detect.text_encoding");
        assert_eq!(MetaColumn::LineEndings.id(), "detect.line_endings");
        assert_eq!(MetaColumn::NativeTags.id(), "native.tags");
    }

    #[test]
    fn extensions_match_the_gating_used_by_extract_column() {
        assert_eq!(MetaColumn::Audio(AudioColumn::Title).extensions(), AUDIO_EXTS);
        assert_eq!(MetaColumn::ImageDimensions.extensions(), IMAGE_EXTS);
        assert_eq!(MetaColumn::DocPages.extensions(), DOC_EXTS);
        assert_eq!(MetaColumn::DocInfo(DocInfoColumn::Title).extensions(), DOC_EXTS);
        assert_eq!(MetaColumn::VideoDuration.extensions(), VIDEO_EXTS);
        assert_eq!(MetaColumn::VideoTag(VideoTagColumn::Title).extensions(), VIDEO_EXTS);
        // The detectors gate on nothing — they run for every extension (applies-to-all sentinel).
        assert!(MetaColumn::TrueType.extensions().is_empty());
        assert!(MetaColumn::TypeMismatch.extensions().is_empty());
        assert!(MetaColumn::TextEncoding.extensions().is_empty());
        assert!(MetaColumn::LineEndings.extensions().is_empty());
        // The native-tags column (CPE-1175) is likewise applies-to-all — OS tags aren't extension-scoped.
        assert!(MetaColumn::NativeTags.extensions().is_empty());
    }

    #[test]
    fn labels_are_family_prefixed_and_disambiguate_recurring_names() {
        // "Year" recurs in both Audio and VideoTag — the family prefix keeps them distinct in a flat list.
        let audio_year = MetaColumn::Audio(AudioColumn::Year).label();
        let video_year = MetaColumn::VideoTag(VideoTagColumn::Year).label();
        assert_ne!(audio_year, video_year);
        assert!(audio_year.starts_with("Audio:"));
        assert!(video_year.starts_with("Video:"));
    }
}
