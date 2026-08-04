//! Metadata-studio dispatch (epic CPE-725): the one entry point the app's metadata commands call to read
//! *all* of a file's editable metadata across the per-format read codecs, and to write edits back for the
//! formats that have a write codec today. Pure — the app adapter supplies the bytes + extension and owns
//! the actual file I/O; this only routes.
//!
//! Read coverage spans the codecs shipped so far: ID3 (mp3), Vorbis (flac / ogg), EXIF (jpeg / tiff), IPTC
//! (jpeg APP13/8BIM/IIM, merged alongside EXIF), XMP (jpeg APP1, merged alongside EXIF + IPTC, plus
//! standalone `.xmp` sidecars), WAV/RIFF-INFO, PDF `/Info`, and MP4/MOV video tags. Write coverage is
//! narrower — only the formats with a write codec ([`crate::media_meta_write`]): **mp3**, **flac**, and
//! **jpeg** (EXIF), **ogg**/**oga** (Vorbis comments), and **wav** (RIFF `LIST`/`INFO`).
//! [`is_writable`] lets the UI show read-only fields for the rest until their writers land (video/PDF
//! write-back are deferred as format-risky, and TIFF EXIF write is deferred because the EXIF *is* a
//! TIFF's own IFD chain).

use crate::media_meta_edit::{apply_edits, MetaEdit, MetaField};
use crate::media_meta_read::{read_exif, read_flac, read_id3v2, read_iptc, read_ogg, read_pdf, read_wav, read_xmp};
use crate::media_meta_write::{write_exif, write_flac, write_id3v2, write_ogg, write_wav};
use crate::video_meta_read::read_mp4;

/// Every metadata field the studio can show for a file, chosen by extension. A file whose kind has no
/// codec (or an unreadable one) yields an empty vec — the studio then shows "no editable metadata".
pub fn read_all(ext: &str, bytes: &[u8]) -> Vec<MetaField> {
    match ext.to_ascii_lowercase().as_str() {
        "mp3" => read_id3v2(bytes),
        "flac" => read_flac(bytes),
        "ogg" | "oga" => read_ogg(bytes),
        "wav" => read_wav(bytes),
        "pdf" => read_pdf(bytes),
        "mp4" | "mov" | "m4v" => read_mp4(bytes),
        "jpg" | "jpeg" => {
            let mut fields = read_exif(bytes);
            fields.extend(read_iptc(bytes));
            fields.extend(read_xmp(bytes));
            fields
        }
        "tif" | "tiff" => read_exif(bytes),
        "xmp" => read_xmp(bytes),
        _ => Vec::new(),
    }
}

/// Whether `ext` has a write-back codec today, so the studio can offer editing (not just viewing).
pub fn is_writable(ext: &str) -> bool {
    // jpg/jpeg carry an EXIF write codec ([`write_exif`]); ogg/oga carry a Vorbis-comment write codec
    // ([`write_ogg`]); wav carries a RIFF LIST/INFO write codec ([`write_wav`]). tif/tiff are
    // intentionally excluded — for a TIFF the EXIF *is* the file's own IFD chain, so rebuilding it from
    // four tags would drop the image.
    matches!(ext.to_ascii_lowercase().as_str(), "mp3" | "flac" | "jpg" | "jpeg" | "ogg" | "oga" | "wav")
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
        "ogg" | "oga" => write_ogg(orig, &result.fields),
        "jpg" | "jpeg" => write_exif(orig, &result.fields),
        "wav" => write_wav(orig, &result.fields),
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
    fn is_writable_for_mp3_flac_jpeg_ogg_and_wav() {
        assert!(is_writable("mp3") && is_writable("FLAC"));
        assert!(is_writable("jpg") && is_writable("JPEG")); // EXIF write codec (CPE-1288)
        assert!(is_writable("ogg") && is_writable("OGA")); // Vorbis-comment write codec (CPE-1289)
        assert!(is_writable("wav") && is_writable("WAV")); // RIFF LIST/INFO write codec (CPE-1298)
        // TIFF EXIF write is deferred (the EXIF is a TIFF's own IFD); other formats have no writer yet.
        assert!(!is_writable("tif") && !is_writable("tiff"));
        assert!(!is_writable("pdf") && !is_writable("mp4"));
    }

    /// Build a minimal WAV: `RIFF` header + a `fmt ` chunk + a `data` chunk carrying `audio`, and (if
    /// `info_fields` is non-empty) a `LIST`/`INFO` chunk built via `write_wav` semantics from those fields.
    fn wav(info_fields: &[(&str, &str)], audio: &[u8]) -> Vec<u8> {
        use crate::media_meta_write::write_wav;
        let fmt_data: [u8; 16] = [1, 0, 2, 0, 0x44, 0xAC, 0, 0, 0, 0, 0, 0, 4, 0, 16, 0];
        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&(fmt_data.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt_data);
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(audio.len() as u32).to_le_bytes());
        body.extend_from_slice(audio);
        if audio.len() % 2 != 0 {
            body.push(0);
        }
        let mut base = Vec::new();
        base.extend_from_slice(b"RIFF");
        base.extend_from_slice(&(body.len() as u32).to_le_bytes());
        base.extend_from_slice(&body);
        if info_fields.is_empty() {
            return base;
        }
        let fields: Vec<MetaField> = info_fields
            .iter()
            .map(|&(k, v)| MetaField { group: "wav".to_string(), key: k.to_string(), value: v.to_string(), editable: true })
            .collect();
        write_wav(&base, &fields).expect("well-formed WAV")
    }

    #[test]
    fn write_back_round_trips_and_preserves_audio_for_wav() {
        let audio = b"WAVEFORMSAMPLESHERE";
        let orig = wav(&[("Title", "Old Title"), ("Artist", "Keep Artist")], audio);
        let edits = vec![
            MetaEdit::Set { group: "wav".into(), key: "Title".into(), value: "New Title".into() },
            MetaEdit::Set { group: "wav".into(), key: "Album".into(), value: "New Album".into() },
        ];
        let out = write_back("wav", &orig, &edits).expect("wav is writable");
        let fields = read_all("wav", &out);
        assert_eq!(get(&fields, "Title"), Some("New Title"));
        assert_eq!(get(&fields, "Album"), Some("New Album"));
        assert_eq!(get(&fields, "Artist"), Some("Keep Artist")); // untouched field survives
        assert!(out.windows(audio.len()).any(|w| w == audio), "audio data must survive byte-for-byte");
    }

    #[test]
    fn write_back_inserts_wav_info_chunk_when_absent() {
        let audio = b"NOINFOAUDIO";
        let orig = wav(&[], audio); // no LIST/INFO chunk at all
        assert!(read_all("wav", &orig).is_empty()); // sanity: nothing to read yet

        let out = write_back("wav", &orig, &[MetaEdit::Set { group: "wav".into(), key: "Title".into(), value: "Fresh".into() }])
            .expect("wav is writable");
        let fields = read_all("wav", &out);
        assert_eq!(get(&fields, "Title"), Some("Fresh"));
        assert!(out.windows(audio.len()).any(|w| w == audio));
    }

    #[test]
    fn write_back_errs_for_malformed_wav_bytes_without_panicking() {
        let err = write_back("wav", b"not a wav file at all", &[]).unwrap_err();
        assert!(err.contains("WAV") || err.contains("RIFF"));
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

    fn exif_set(key: &str, value: &str) -> MetaEdit {
        MetaEdit::Set { group: "exif".into(), key: key.into(), value: value.into() }
    }

    #[test]
    fn write_back_round_trips_exif_for_jpeg_and_preserves_other_segments() {
        // Seed a JPEG whose only non-SOI content is a COM segment + EOI — our preservation witness.
        let com = b"non-exif-comment";
        let mut seed = vec![0xFFu8, 0xD8];
        seed.extend_from_slice(&[0xFF, 0xFE]);
        seed.extend_from_slice(&((com.len() + 2) as u16).to_be_bytes());
        seed.extend_from_slice(com);
        seed.extend_from_slice(&[0xFF, 0xD9]);
        let tail = seed[2..].to_vec(); // COM + EOI

        // Build a base image carrying two editable EXIF tags via the write path itself.
        let base = write_back("jpg", &seed, &[exif_set("ImageDescription", "Old Caption"), exif_set("Artist", "Old Artist")])
            .expect("jpg is writable");
        let fields = read_all("jpg", &base);
        assert_eq!(get(&fields, "ImageDescription"), Some("\"Old Caption\"")); // reader quotes ASCII
        assert_eq!(get(&fields, "Artist"), Some("\"Old Artist\""));
        assert!(base.ends_with(&tail[..])); // non-EXIF segments intact

        // Edit only ImageDescription; Artist must survive the rebuild unchanged.
        let out = write_back("jpg", &base, &[exif_set("ImageDescription", "New Caption")]).unwrap();
        let after = read_all("jpg", &out);
        assert_eq!(get(&after, "ImageDescription"), Some("\"New Caption\""));
        assert_eq!(get(&after, "Artist"), Some("\"Old Artist\""));
        assert!(out.ends_with(&tail[..])); // still preserved — only the APP1 was replaced
        assert!(is_writable("jpg"));
    }

    #[test]
    fn write_back_errors_for_non_jpeg_bytes_routed_to_the_exif_codec() {
        let err = write_back("jpg", b"not a jpeg at all", &[exif_set("ImageDescription", "x")]).unwrap_err();
        assert!(err.contains("JPEG") || err.contains("SOI"));
    }
}
