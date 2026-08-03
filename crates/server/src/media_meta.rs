//! Metadata-studio dispatch (epic CPE-725): the one entry point the app's metadata commands call to read
//! *all* of a file's editable metadata across the per-format read codecs, and to write edits back for the
//! formats that have a write codec today. Pure — the app adapter supplies the bytes + extension and owns
//! the actual file I/O; this only routes.
//!
//! Read coverage spans the codecs shipped so far: ID3 (mp3), Vorbis (flac / ogg), EXIF (jpeg / tiff), IPTC
//! (jpeg APP13/8BIM/IIM, merged alongside EXIF), PDF `/Info`, and MP4/MOV video tags. Write coverage is
//! narrower — only the formats with a write codec
//! ([`crate::media_meta_write`]): **mp3** and **flac**. [`is_writable`] lets the UI show read-only fields
//! for the rest until their writers land (OGG/EXIF/video/PDF write-back are deferred as format-risky).

use crate::media_meta_edit::{apply_edits, MetaEdit, MetaField};
use crate::media_meta_read::{read_exif, read_flac, read_id3v2, read_iptc, read_ogg, read_pdf};
use crate::media_meta_write::{write_flac, write_id3v2};
use crate::video_meta_read::read_mp4;

/// Every metadata field the studio can show for a file, chosen by extension. A file whose kind has no
/// codec (or an unreadable one) yields an empty vec — the studio then shows "no editable metadata".
pub fn read_all(ext: &str, bytes: &[u8]) -> Vec<MetaField> {
    match ext.to_ascii_lowercase().as_str() {
        "mp3" => read_id3v2(bytes),
        "flac" => read_flac(bytes),
        "ogg" | "oga" => read_ogg(bytes),
        "pdf" => read_pdf(bytes),
        "mp4" | "mov" | "m4v" => read_mp4(bytes),
        "jpg" | "jpeg" => {
            let mut fields = read_exif(bytes);
            fields.extend(read_iptc(bytes));
            fields
        }
        "tif" | "tiff" => read_exif(bytes),
        _ => Vec::new(),
    }
}

/// Whether `ext` has a write-back codec today, so the studio can offer editing (not just viewing).
pub fn is_writable(ext: &str) -> bool {
    matches!(ext.to_ascii_lowercase().as_str(), "mp3" | "flac")
}

/// Apply `edits` to the file's current fields and serialise the result back to new file bytes. Reads the
/// current fields itself (so the write codec rebuilds the complete tag), applies the edit policy
/// ([`apply_edits`] — read-only fields are refused), then re-serialises with the format's write codec.
/// Returns `Err` with a friendly message for a format that has no writer yet.
pub fn write_back(ext: &str, orig: &[u8], edits: &[MetaEdit]) -> Result<Vec<u8>, String> {
    let fields = read_all(ext, orig);
    let result = apply_edits(&fields, edits);
    match ext.to_ascii_lowercase().as_str() {
        "mp3" => Ok(write_id3v2(orig, &result.fields)),
        "flac" => Ok(write_flac(orig, &result.fields)),
        other => Err(format!("editing {other} metadata isn't supported yet")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal ID3v2.3 tag from `(4-char id, latin1 text)` frames, followed by fake audio bytes.
    fn mp3(frames: &[(&str, &str)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (id, text) in frames {
            let mut fb = vec![0u8]; // latin-1 encoding byte
            fb.extend_from_slice(text.as_bytes());
            body.extend_from_slice(id.as_bytes());
            body.extend_from_slice(&(fb.len() as u32).to_be_bytes());
            body.extend_from_slice(&[0, 0]);
            body.extend_from_slice(&fb);
        }
        let syncsafe = |mut v: u32| {
            let mut o = [0u8; 4];
            for i in (0..4).rev() {
                o[i] = (v & 0x7F) as u8;
                v >>= 7;
            }
            o
        };
        let mut t = Vec::new();
        t.extend_from_slice(b"ID3");
        t.extend_from_slice(&[3, 0, 0]);
        t.extend_from_slice(&syncsafe(body.len() as u32));
        t.extend_from_slice(&body);
        t.extend_from_slice(b"\xFF\xFBFAKEAUDIO"); // audio payload
        t
    }

    fn get<'a>(fields: &'a [MetaField], key: &str) -> Option<&'a str> {
        fields.iter().find(|f| f.key == key).map(|f| f.value.as_str())
    }

    #[test]
    fn read_all_routes_by_extension() {
        let tag = mp3(&[("TIT2", "Song"), ("TPE1", "Band")]);
        assert_eq!(get(&read_all("mp3", &tag), "Title"), Some("Song"));
        assert_eq!(get(&read_all("MP3", &tag), "Artist"), Some("Band")); // case-insensitive
        // Wrong extension → not parsed as that kind → empty.
        assert!(read_all("pdf", &tag).is_empty());
        assert!(read_all("xyz", b"whatever").is_empty());
    }

    #[test]
    fn is_writable_only_for_mp3_and_flac() {
        assert!(is_writable("mp3") && is_writable("FLAC"));
        assert!(!is_writable("pdf") && !is_writable("mp4") && !is_writable("ogg") && !is_writable("jpg"));
    }

    #[test]
    fn write_back_round_trips_and_preserves_audio_for_mp3() {
        let orig = mp3(&[("TIT2", "Old Title"), ("TPE1", "Keep Artist")]);
        let edits = vec![
            MetaEdit::Set { group: "id3".into(), key: "Title".into(), value: "New Title".into() },
            MetaEdit::Set { group: "id3".into(), key: "Album".into(), value: "New Album".into() },
        ];
        let out = write_back("mp3", &orig, &edits).expect("mp3 is writable");
        let fields = read_all("mp3", &out);
        assert_eq!(get(&fields, "Title"), Some("New Title"));
        assert_eq!(get(&fields, "Album"), Some("New Album"));
        assert_eq!(get(&fields, "Artist"), Some("Keep Artist")); // untouched field survives
        assert!(out.ends_with(b"\xFF\xFBFAKEAUDIO")); // audio preserved byte-for-byte
    }

    #[test]
    fn write_back_errors_for_unsupported_format() {
        let err = write_back("pdf", b"%PDF-1.4", &[]).unwrap_err();
        assert!(err.contains("pdf"));
    }
}
