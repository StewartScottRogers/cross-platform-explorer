//! Metadata-column dispatcher (CPE-975, epic CPE-707): the single entry point that turns a file's bytes +
//! extension + a requested column into a typed [`CellValue`], routing to the right per-family extractor.
//!
//! This unifies the pieces the details-view column system needs: the audio read codecs
//! ([`crate::media_meta_read`]: ID3 / FLAC / OGG) feeding the audio column typing
//! ([`crate::media_column`]), and the image header reader ([`crate::image_column`]). A caller (the column
//! UI, an MCP tool, a command) picks a [`MetaColumn`] and hands over the file's leading bytes; this decides
//! the codec by extension and returns the cell — or [`CellValue::Empty`] when the file kind doesn't match
//! the column (so it sorts last). Pure: no filesystem, the adapter reads the bytes.

use crate::image_column::image_dimensions_cell;
use crate::media_column::{audio_cell, AudioColumn};
use crate::media_meta_edit::MetaField;
use crate::media_meta_read::{read_flac, read_id3v2, read_ogg};
use crate::metadata_column::CellValue;

/// A metadata column the details view can add, spanning media families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaColumn {
    /// A typed audio-tag column (Title/Artist/Track/Year/…), read from ID3/FLAC/OGG by extension.
    Audio(AudioColumn),
    /// The image's pixel dimensions (`w × h`, sorted by area).
    ImageDimensions,
}

/// Read a file's audio tags, choosing the codec by extension: `mp3` → ID3v2, `flac` → FLAC/Vorbis,
/// `ogg`/`oga` → OGG/Vorbis. A non-audio (or unrecognised) extension yields no fields.
pub fn read_audio_tags(ext: &str, bytes: &[u8]) -> Vec<MetaField> {
    match ext.to_ascii_lowercase().as_str() {
        "mp3" => read_id3v2(bytes),
        "flac" => read_flac(bytes),
        "ogg" | "oga" => read_ogg(bytes),
        _ => Vec::new(),
    }
}

/// Whether `ext` is an image kind the dimensions reader should attempt (avoids decoding non-images).
fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff" | "tif"
    )
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

    fn ogg(comments: &[&str]) -> Vec<u8> {
        let mut o = Vec::new();
        o.extend_from_slice(b"OggS");
        o.extend_from_slice(&[0u8; 22]);
        o.extend_from_slice(b"\x01\xff\x03vorbis");
        o.extend_from_slice(&vorbis_block(comments));
        o
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::new(w, h);
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
        buf
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

    #[test]
    fn read_audio_tags_dispatches_and_is_empty_for_non_audio() {
        assert!(!read_audio_tags("mp3", &id3(&[("TIT2", "X")])).is_empty());
        assert!(read_audio_tags("png", &png(2, 2)).is_empty());
        assert!(read_audio_tags("", b"").is_empty());
    }
}
