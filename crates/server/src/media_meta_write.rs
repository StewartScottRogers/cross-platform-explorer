//! Media-metadata **write codec** (CPE-1035, epic CPE-725): the counterpart to
//! [`crate::media_meta_read::read_id3v2`]. [`crate::media_meta_edit`] owns the edit *policy* over
//! [`MetaField`]s but does no file parsing/writing — "the codec layer reads the fields in and writes the
//! result back." [`crate::media_meta_read`] is that read layer for ID3v2; this module is the write layer:
//! it builds a fresh, valid **ID3v2.4** tag from the `group == "id3"` fields and prepends it to the
//! original audio payload, replacing any pre-existing tag rather than stacking on top of it.
//!
//! Pure + std-only (no new deps): fully cargo-testable by round-tripping through
//! [`crate::media_meta_read::read_id3v2`] with synthesised inputs, and does no I/O. Bounds-checked
//! throughout — malformed/short/empty input never panics; at worst the whole input is treated as the
//! audio payload with no pre-existing tag to strip.

use crate::media_meta_edit::MetaField;

/// Build an ID3v2.4 tag from the `"id3"`-group fields in `fields` and prepend it to the audio payload of
/// `orig` — i.e. `orig`'s bytes *after* any pre-existing ID3v2 tag, so calling this again on the output is
/// idempotent (the old tag is replaced, never stacked). Fields outside the `"id3"` group are ignored.
/// Never panics, even on empty/short/garbage `orig`.
pub fn write_id3v2(orig: &[u8], fields: &[MetaField]) -> Vec<u8> {
    let payload = strip_existing_tag(orig);
    let body = build_frames(fields);

    let mut out = Vec::with_capacity(10 + body.len() + payload.len());
    out.extend_from_slice(b"ID3");
    out.extend_from_slice(&[0x04, 0x00]); // major=4, revision=0
    out.push(0x00); // flags: no unsynchronisation, no extended header, no experimental, no footer
    out.extend_from_slice(&syncsafe28_encode(body.len() as u32));
    out.extend_from_slice(&body);
    out.extend_from_slice(payload);
    out
}

/// Return the slice of `orig` that follows any pre-existing ID3v2 header+tag. If `orig` doesn't start with
/// a well-formed `ID3` header (too short, wrong magic), the whole input is treated as payload — nothing to
/// strip. The declared tag size is clamped to the buffer length so a lying/corrupt size can't panic.
fn strip_existing_tag(orig: &[u8]) -> &[u8] {
    if orig.len() < 10 || &orig[0..3] != b"ID3" {
        return orig;
    }
    let tag_size = syncsafe28_decode(&orig[6..10]) as usize;
    let start = (10usize + tag_size).min(orig.len());
    &orig[start..]
}

/// Concatenate one text/COMM frame per recognised, non-empty `"id3"`-group field. Frame order follows
/// `fields`' own order. Unrecognised friendly keys that aren't themselves a valid raw 4-char `T...` frame
/// id are skipped (nothing useful to write). Blank values are skipped too — a blank field should be
/// cleared via `media_meta_edit`, not written as an empty frame.
fn build_frames(fields: &[MetaField]) -> Vec<u8> {
    let mut body = Vec::new();
    for f in fields {
        if f.group != "id3" || f.value.is_empty() {
            continue;
        }
        let Some(id) = frame_id(&f.key) else { continue };
        if id == "COMM" {
            body.extend_from_slice(&encode_comm_frame(&f.value));
        } else {
            body.extend_from_slice(&encode_text_frame(&id, &f.value));
        }
    }
    body
}

/// Map a friendly key (as produced by [`crate::media_meta_read::friendly_key`]) back to its raw ID3v2.4
/// frame id. A key that isn't one of the known friendly names is written as-is if it's already a
/// plausible raw frame id (4 ASCII alphanumerics starting with `T`, or exactly `COMM`) — this mirrors the
/// reader passing an unknown text frame through under its raw id. Anything else is skipped.
fn frame_id(key: &str) -> Option<String> {
    let raw = match key {
        "Title" => "TIT2",
        "Artist" => "TPE1",
        "Album" => "TALB",
        "Album Artist" => "TPE2",
        "Track" => "TRCK",
        "Disc" => "TPOS",
        "Genre" => "TCON",
        "Year" => "TDRC",
        "Date" => "TDAT",
        "Composer" => "TCOM",
        "Publisher" => "TPUB",
        "BPM" => "TBPM",
        "Copyright" => "TCOP",
        "Encoder" => "TENC",
        "Comment" => "COMM",
        other => {
            let is_raw_text_id = other.len() == 4 && other.starts_with('T') && other.chars().all(|c| c.is_ascii_alphanumeric());
            if is_raw_text_id || other == "COMM" {
                other
            } else {
                return None;
            }
        }
    };
    Some(raw.to_string())
}

/// Encode a text frame (`T...`): id + syncsafe28(body len) + flags(0,0) + encoding byte (UTF-8) + text.
fn encode_text_frame(id: &str, value: &str) -> Vec<u8> {
    let mut frame_body = Vec::with_capacity(1 + value.len());
    frame_body.push(0x03); // UTF-8
    frame_body.extend_from_slice(value.as_bytes());
    frame_header_and_body(id, frame_body)
}

/// Encode a `COMM` comment frame: id + syncsafe28(body len) + flags(0,0) + encoding(UTF-8) + language
/// (`"eng"`) + empty description terminator (`\0`) + the comment text.
fn encode_comm_frame(value: &str) -> Vec<u8> {
    let mut frame_body = Vec::with_capacity(1 + 3 + 1 + value.len());
    frame_body.push(0x03); // UTF-8
    frame_body.extend_from_slice(b"eng");
    frame_body.push(0x00); // empty description, NUL-terminated
    frame_body.extend_from_slice(value.as_bytes());
    frame_header_and_body("COMM", frame_body)
}

/// Common frame serialization: 4-char id + syncsafe28(len) + 2 zero flag bytes + the body.
fn frame_header_and_body(id: &str, frame_body: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(10 + frame_body.len());
    out.extend_from_slice(id.as_bytes());
    out.extend_from_slice(&syncsafe28_encode(frame_body.len() as u32));
    out.extend_from_slice(&[0x00, 0x00]); // frame flags
    out.extend_from_slice(&frame_body);
    out
}

/// Encode a value as a 4-byte syncsafe28 integer (each byte carries 7 bits, high bit always clear) — the
/// ID3v2.4 header/frame-size encoding. Values above 2^28-1 are truncated to fit (not expected in practice
/// for a metadata tag).
fn syncsafe28_encode(mut v: u32) -> [u8; 4] {
    v &= 0x0FFF_FFFF;
    let mut out = [0u8; 4];
    for i in (0..4).rev() {
        out[i] = (v & 0x7F) as u8;
        v >>= 7;
    }
    out
}

/// Decode a 4-byte syncsafe28 integer (mirrors [`crate::media_meta_read`]'s private helper of the same
/// shape; re-implemented locally to avoid touching that module's visibility).
fn syncsafe28_decode(b: &[u8]) -> u32 {
    ((b[0] as u32 & 0x7F) << 21) | ((b[1] as u32 & 0x7F) << 14) | ((b[2] as u32 & 0x7F) << 7) | (b[3] as u32 & 0x7F)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_meta_read::read_id3v2;

    fn field(key: &str, value: &str) -> MetaField {
        MetaField { group: "id3".to_string(), key: key.to_string(), value: value.to_string(), editable: true }
    }

    fn get<'a>(fields: &'a [MetaField], key: &str) -> Option<&'a str> {
        fields.iter().find(|f| f.key == key).map(|f| f.value.as_str())
    }

    #[test]
    fn round_trips_representative_fields_with_no_existing_tag() {
        let audio = b"\xFF\xFB\x90\x00some mp3 frame data".to_vec();
        let fields = vec![
            field("Title", "Bohemian Rhapsody"),
            field("Artist", "Queen"),
            field("Album", "A Night at the Opera"),
            field("Track", "11/12"),
            field("Year", "1975"),
            field("Comment", "Remastered 2011"),
        ];
        let written = write_id3v2(&audio, &fields);

        assert!(written.starts_with(b"ID3\x04\x00"));
        // Audio payload preserved byte-for-byte after the new tag.
        assert!(written.ends_with(&audio[..]));

        let read_back = read_id3v2(&written);
        assert_eq!(get(&read_back, "Title"), Some("Bohemian Rhapsody"));
        assert_eq!(get(&read_back, "Artist"), Some("Queen"));
        assert_eq!(get(&read_back, "Album"), Some("A Night at the Opera"));
        assert_eq!(get(&read_back, "Track"), Some("11/12"));
        assert_eq!(get(&read_back, "Year"), Some("1975"));
        assert_eq!(get(&read_back, "Comment"), Some("Remastered 2011"));
    }

    #[test]
    fn replaces_a_v23_tag_rather_than_stacking_on_it() {
        // Build a minimal v2.3 tag (plain big-endian frame sizes) with an old Title, followed by audio.
        let old_title = b"Old Title";
        let mut old_frame_body = vec![0x00]; // Latin-1 encoding
        old_frame_body.extend_from_slice(old_title);
        let mut old_tag_body = Vec::new();
        old_tag_body.extend_from_slice(b"TIT2");
        old_tag_body.extend_from_slice(&(old_frame_body.len() as u32).to_be_bytes());
        old_tag_body.extend_from_slice(&[0, 0]);
        old_tag_body.extend_from_slice(&old_frame_body);

        let mut old_tag = Vec::new();
        old_tag.extend_from_slice(b"ID3");
        old_tag.extend_from_slice(&[0x03, 0x00, 0x00]); // v2.3, no flags
        old_tag.extend_from_slice(&syncsafe28_encode(old_tag_body.len() as u32));
        old_tag.extend_from_slice(&old_tag_body);

        let audio = b"AUDIOBYTES".to_vec();
        let mut orig = old_tag.clone();
        orig.extend_from_slice(&audio);

        // Sanity: the reader does see the old v2.3 tag's title before we touch anything.
        let pre = read_id3v2(&orig);
        assert_eq!(get(&pre, "Title"), Some("Old Title"));

        let written = write_id3v2(&orig, &[field("Title", "New Title")]);

        // New tag is v2.4, old tag gone (not stacked before/after it), audio payload preserved exactly.
        assert!(written.starts_with(b"ID3\x04\x00"));
        assert!(written.ends_with(&audio[..]));
        let read_back = read_id3v2(&written);
        assert_eq!(get(&read_back, "Title"), Some("New Title"));

        // The old tag's bytes must not appear anywhere verbatim in the output (no stacking).
        assert!(!contains_subslice(&written, &old_tag));
    }

    #[test]
    fn writing_twice_is_idempotent_payload_wise() {
        let audio = b"raw-audio-bytes-here".to_vec();
        let fields = vec![field("Title", "A"), field("Artist", "B")];
        let once = write_id3v2(&audio, &fields);
        let twice = write_id3v2(&once, &fields);
        assert_eq!(once, twice, "re-writing over an already-written tag must produce identical bytes");
    }

    #[test]
    fn non_id3_fields_are_ignored() {
        let audio = b"payload".to_vec();
        let fields = vec![
            field("Title", "Kept"),
            MetaField { group: "exif".to_string(), key: "Make".to_string(), value: "Acme".to_string(), editable: false },
            MetaField { group: "vorbis".to_string(), key: "Title".to_string(), value: "Ignored".to_string(), editable: true },
        ];
        let written = write_id3v2(&audio, &fields);
        let read_back = read_id3v2(&written);
        assert_eq!(read_back.len(), 1);
        assert_eq!(get(&read_back, "Title"), Some("Kept"));
    }

    #[test]
    fn never_panics_on_empty_short_or_garbage_orig() {
        for orig in [
            &b""[..],
            &b"I"[..],
            &b"ID3"[..],
            &b"ID3\x04"[..],
            &b"ID3\x04\x00\x00\x00\x00\x00\x00"[..], // exactly 10 bytes, tag_size 0
            &b"not an id3 tag at all, just noise"[..],
            &[0xFFu8, 0xFB, 0x90, 0x00][..],
        ] {
            let out = write_id3v2(orig, &[field("Title", "X")]);
            assert!(out.starts_with(b"ID3\x04\x00"));
        }
    }

    #[test]
    fn unrecognised_friendly_key_without_raw_id_form_is_skipped() {
        let audio = b"payload".to_vec();
        let fields = vec![field("Some Random Thing", "value"), field("Title", "Kept")];
        let written = write_id3v2(&audio, &fields);
        let read_back = read_id3v2(&written);
        assert_eq!(read_back.len(), 1);
        assert_eq!(get(&read_back, "Title"), Some("Kept"));
    }

    #[test]
    fn raw_four_char_text_frame_id_passes_through() {
        // Mirrors the reader's behaviour: an unknown text frame id like "TSSE" round-trips under itself.
        let audio = b"payload".to_vec();
        let fields = vec![field("TSSE", "LAME 3.100")];
        let written = write_id3v2(&audio, &fields);
        let read_back = read_id3v2(&written);
        assert_eq!(get(&read_back, "TSSE"), Some("LAME 3.100"));
    }

    #[test]
    fn blank_value_fields_are_skipped() {
        let audio = b"payload".to_vec();
        let fields = vec![field("Title", ""), field("Artist", "Someone")];
        let written = write_id3v2(&audio, &fields);
        let read_back = read_id3v2(&written);
        assert_eq!(get(&read_back, "Title"), None);
        assert_eq!(get(&read_back, "Artist"), Some("Someone"));
    }

    #[test]
    fn non_ascii_utf8_values_round_trip() {
        let audio = b"payload".to_vec();
        let fields = vec![field("Artist", "Sigur Rós"), field("Title", "Étude")];
        let written = write_id3v2(&audio, &fields);
        let read_back = read_id3v2(&written);
        assert_eq!(get(&read_back, "Artist"), Some("Sigur Rós"));
        assert_eq!(get(&read_back, "Title"), Some("Étude"));
    }

    #[test]
    fn syncsafe_round_trips() {
        for v in [0u32, 1, 127, 128, 16383, 16384, 0x0FFF_FFFF] {
            let encoded = syncsafe28_encode(v);
            assert_eq!(syncsafe28_decode(&encoded), v);
        }
    }

    fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
    }
}
