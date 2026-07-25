//! Media-metadata **read codecs** (CPE-970, epic CPE-725; also feeds CPE-707 columns).
//!
//! [`crate::media_meta_edit`] (CPE-942) owns the edit *policy* over [`MetaField`]s but deliberately does no
//! file parsing — "the codec layer reads the fields in and writes the result back." This module is that
//! read layer. It starts with [`read_id3v2`]: parse the ubiquitous ID3v2 audio tag (MP3 and friends) into
//! the existing [`MetaField`] model, so the studio can show/edit real tags and a column extractor can
//! surface Artist / Album / Title / Track.
//!
//! Pure + std-only (no new deps): the input is the file's leading bytes, so it is fully cargo-testable with
//! synthesised tags and does no I/O — the adapter reads the bytes and dispatches by extension. Every read is
//! bounds-checked, so malformed/truncated input yields a sane partial result and **never panics**.

use crate::media_meta_edit::MetaField;

/// Parse an ID3v2 tag (v2.2 / v2.3 / v2.4) from the start of `bytes` into editable [`MetaField`]s in group
/// `"id3"`. Returns an empty vec when there is no `ID3` header. Malformed frames are skipped rather than
/// failing the whole parse.
pub fn read_id3v2(bytes: &[u8]) -> Vec<MetaField> {
    // Header: "ID3" + major(1) + revision(1) + flags(1) + syncsafe size(4) = 10 bytes.
    if bytes.len() < 10 || &bytes[0..3] != b"ID3" {
        return Vec::new();
    }
    let major = bytes[3];
    if !(2..=4).contains(&major) {
        return Vec::new(); // unknown ID3v2 generation
    }
    let flags = bytes[5];
    let tag_size = syncsafe28(&bytes[6..10]) as usize;
    let mut pos = 10usize;

    // An extended header (flags bit 0x40) precedes the frames — skip it best-effort. Its first 4 bytes are
    // its own size (syncsafe in v2.4, plain in v2.3). If that would run past the buffer, bail cleanly.
    if flags & 0x40 != 0 && bytes.len() >= pos + 4 {
        let ext_size = if major >= 4 {
            syncsafe28(&bytes[pos..pos + 4]) as usize
        } else {
            be_u32(&bytes[pos..pos + 4]) as usize + 4 // v2.3 size excludes the 4 size bytes
        };
        pos = pos.saturating_add(ext_size);
    }

    // Frames live in [10, 10 + tag_size), clamped to the actual buffer (a lying size can't over-read).
    let end = (10 + tag_size).min(bytes.len());
    let mut fields = Vec::new();

    let (id_len, size_len) = if major == 2 { (3usize, 3usize) } else { (4usize, 4usize) };
    let flags_len = if major == 2 { 0usize } else { 2usize };
    let header_len = id_len + size_len + flags_len;

    while pos + header_len <= end {
        let id = &bytes[pos..pos + id_len];
        // A NUL id means we've hit the padding that follows the last frame — stop.
        if id[0] == 0 {
            break;
        }
        let frame_size = match major {
            2 => be_u24(&bytes[pos + id_len..pos + id_len + size_len]) as usize,
            4 => syncsafe28(&bytes[pos + id_len..pos + id_len + size_len]) as usize,
            _ => be_u32(&bytes[pos + id_len..pos + id_len + size_len]) as usize,
        };
        let body_start = pos + header_len;
        let body_end = body_start + frame_size;
        // A zero or out-of-range size is corrupt — stop rather than spin.
        if frame_size == 0 || body_end > end {
            break;
        }
        let id_str: String = id.iter().map(|&b| b as char).collect();
        let body = &bytes[body_start..body_end];
        if let Some(field) = decode_frame(&id_str, body) {
            fields.push(field);
        }
        pos = body_end;
    }

    fields
}

/// Turn one frame into a [`MetaField`], or `None` if it's a kind we don't surface. Text frames (`T…`) and
/// `COMM` comments are decoded; everything else (pictures, private, etc.) is skipped.
fn decode_frame(id: &str, body: &[u8]) -> Option<MetaField> {
    let key = friendly_key(id)?;
    let value = if id == "COMM" || id == "COM" {
        decode_comment(body)?
    } else if id.starts_with('T') {
        // Text frame: first byte is the encoding, the rest is the string.
        let (enc, text) = body.split_first()?;
        decode_text(*enc, text)
    } else {
        return None;
    };
    let value = value.trim_matches('\u{0}').trim().to_string();
    if value.is_empty() {
        return None;
    }
    Some(MetaField { group: "id3".to_string(), key, value, editable: true })
}

/// A `COMM`/`COM` comment: encoding(1) + language(3) + short description (NUL-terminated in that encoding) +
/// the comment text. We keep only the text.
fn decode_comment(body: &[u8]) -> Option<String> {
    let (enc, rest) = body.split_first()?;
    let rest = rest.get(3..)?; // skip the 3-byte language code
    // The description is terminated by NUL (one byte for Latin-1/UTF-8, two for UTF-16). Find it, then the
    // remainder is the comment text.
    let text = if *enc == 1 || *enc == 2 {
        // UTF-16: scan for a 16-bit NUL on an even boundary.
        let mut i = 0;
        while i + 1 < rest.len() {
            if rest[i] == 0 && rest[i + 1] == 0 {
                break;
            }
            i += 2;
        }
        rest.get(i + 2..).unwrap_or(&[])
    } else {
        let i = rest.iter().position(|&b| b == 0).map(|p| p + 1).unwrap_or(rest.len());
        rest.get(i..).unwrap_or(&[])
    };
    Some(decode_text(*enc, text))
}

/// Decode ID3 text-frame bytes per the encoding byte: 0=Latin-1, 1=UTF-16 w/ BOM, 2=UTF-16BE, 3=UTF-8.
/// Unknown encodings fall back to a lossy UTF-8 read. Trailing NULs are left for the caller to trim.
fn decode_text(encoding: u8, data: &[u8]) -> String {
    match encoding {
        0 => data.iter().map(|&b| b as char).collect(), // Latin-1: each byte is a codepoint
        1 => decode_utf16_bom(data),
        2 => decode_utf16(data, false), // UTF-16BE, no BOM
        _ => String::from_utf8_lossy(data).into_owned(), // 3 = UTF-8, and a safe default
    }
}

/// UTF-16 with a leading byte-order mark (`FF FE` = little-endian, `FE FF` = big-endian). No/short BOM →
/// assume little-endian, the common case.
fn decode_utf16_bom(data: &[u8]) -> String {
    match data {
        [0xFF, 0xFE, rest @ ..] => decode_utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => decode_utf16(rest, false),
        _ => decode_utf16(data, true),
    }
}

/// Decode raw UTF-16 code units (`little_endian` picks the byte order), lossily for unpaired surrogates.
fn decode_utf16(data: &[u8], little_endian: bool) -> String {
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| if little_endian { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Map an ID3 frame id (v2.3/v2.4 4-char, or v2.2 3-char) to a friendly key. Known frames get a human name;
/// an unrecognised text frame passes through under its raw id (so nothing useful is dropped).
fn friendly_key(id: &str) -> Option<String> {
    let named = match id {
        "TIT2" | "TT2" => "Title",
        "TPE1" | "TP1" => "Artist",
        "TALB" | "TAL" => "Album",
        "TPE2" | "TP2" => "Album Artist",
        "TRCK" | "TRK" => "Track",
        "TPOS" | "TPA" => "Disc",
        "TCON" | "TCO" => "Genre",
        "TYER" | "TYE" | "TDRC" => "Year",
        "TDAT" | "TDA" => "Date",
        "TCOM" | "TCM" => "Composer",
        "TPUB" | "TPB" => "Publisher",
        "TBPM" | "TBP" => "BPM",
        "TCOP" | "TCR" => "Copyright",
        "TENC" | "TEN" => "Encoder",
        "COMM" | "COM" => "Comment",
        // Any other text frame: keep it under its raw id rather than silently dropping.
        other if other.starts_with('T') => return Some(other.to_string()),
        _ => return None,
    };
    Some(named.to_string())
}

/// Read a 28-bit ID3 "syncsafe" integer from 4 bytes (each contributes its low 7 bits, high bit ignored).
fn syncsafe28(b: &[u8]) -> u32 {
    ((b[0] as u32 & 0x7F) << 21)
        | ((b[1] as u32 & 0x7F) << 14)
        | ((b[2] as u32 & 0x7F) << 7)
        | (b[3] as u32 & 0x7F)
}

/// Read a big-endian u32 from 4 bytes.
fn be_u32(b: &[u8]) -> u32 {
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | (b[3] as u32)
}

/// Read a big-endian u24 from 3 bytes (v2.2 frame sizes).
fn be_u24(b: &[u8]) -> u32 {
    ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32)
}

/// Read a little-endian u32 from 4 bytes (Vorbis-comment lengths/counts).
fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

// ---- FLAC / Vorbis comments (CPE-972) ----

/// Parse a FLAC stream's Vorbis-comment tags into [`MetaField`]s (group `"vorbis"`). Returns empty when the
/// `fLaC` magic is absent or there is no comment block. Reuses [`parse_vorbis_comment`] for the block body.
pub fn read_flac(bytes: &[u8]) -> Vec<MetaField> {
    if bytes.len() < 4 || &bytes[0..4] != b"fLaC" {
        return Vec::new();
    }
    // FLAC metadata blocks follow the magic: a 1-byte header (bit7 = last-block flag, bits0-6 = block type)
    // + a 3-byte big-endian length + the block data. The Vorbis comment is block type 4.
    let mut pos = 4usize;
    while pos + 4 <= bytes.len() {
        let header = bytes[pos];
        let is_last = header & 0x80 != 0;
        let block_type = header & 0x7F;
        let len = be_u24(&bytes[pos + 1..pos + 4]) as usize;
        let data_start = pos + 4;
        let data_end = data_start + len;
        if data_end > bytes.len() {
            break; // truncated block — stop rather than over-read
        }
        if block_type == 4 {
            return parse_vorbis_comment(&bytes[data_start..data_end]);
        }
        if is_last {
            break;
        }
        pos = data_end;
    }
    Vec::new()
}

/// Parse an OGG stream's Vorbis-comment tags into [`MetaField`]s (group `"vorbis"`). Returns empty when the
/// `OggS` magic is absent or no comment header is found. The Vorbis *comment header* packet always begins
/// with the 7-byte signature `\x03vorbis`; we locate it and hand the bytes that follow to
/// [`parse_vorbis_comment`], which reads exactly its declared entries and ignores the trailing framing.
///
/// This is a pragmatic reader: it assumes the comment header isn't split across Ogg pages (true for typical
/// tag sizes). Full multi-page packet reassembly is a later refinement; the shared [`parse_vorbis_comment`]
/// stays the codec.
pub fn read_ogg(bytes: &[u8]) -> Vec<MetaField> {
    if bytes.len() < 4 || &bytes[0..4] != b"OggS" {
        return Vec::new();
    }
    const SIG: &[u8] = b"\x03vorbis";
    let Some(idx) = find_subslice(bytes, SIG) else { return Vec::new() };
    parse_vorbis_comment(&bytes[idx + SIG.len()..])
}

/// The index of the first occurrence of `needle` in `haystack`, or `None`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parse a raw Vorbis-comment block into [`MetaField`]s. Layout (all lengths little-endian): vendor length +
/// vendor string, comment count, then each as length + `KEY=VALUE` (UTF-8). Malformed/truncated input stops
/// the parse with whatever was read — never a panic. Shared by FLAC and (later) OGG.
pub fn parse_vorbis_comment(block: &[u8]) -> Vec<MetaField> {
    let mut fields = Vec::new();
    // vendor string
    if block.len() < 4 {
        return fields;
    }
    let vendor_len = le_u32(&block[0..4]) as usize;
    let mut pos = 4 + vendor_len;
    if pos + 4 > block.len() {
        return fields;
    }
    let count = le_u32(&block[pos..pos + 4]) as usize;
    pos += 4;
    for _ in 0..count {
        if pos + 4 > block.len() {
            break;
        }
        let len = le_u32(&block[pos..pos + 4]) as usize;
        pos += 4;
        let Some(raw) = block.get(pos..pos + len) else { break };
        pos += len;
        let Ok(text) = std::str::from_utf8(raw) else { continue };
        let Some((key, value)) = text.split_once('=') else { continue };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if let Some(friendly) = vorbis_key(key) {
            fields.push(MetaField { group: "vorbis".to_string(), key: friendly, value: value.to_string(), editable: true });
        }
    }
    fields
}

/// Map a Vorbis-comment field name (case-insensitive) to a friendly key — the **same** names the ID3 codec
/// emits, so a downstream column/studio consumer treats FLAC and MP3 tags identically. An unrecognised key
/// passes through capitalised as-is (nothing useful dropped).
fn vorbis_key(name: &str) -> Option<String> {
    let friendly = match name.to_ascii_uppercase().as_str() {
        "TITLE" => "Title",
        "ARTIST" => "Artist",
        "ALBUM" => "Album",
        "ALBUMARTIST" | "ALBUM ARTIST" => "Album Artist",
        "TRACKNUMBER" | "TRACKNUM" | "TRACK" => "Track",
        "DISCNUMBER" | "DISC" => "Disc",
        "GENRE" => "Genre",
        "DATE" | "YEAR" => "Year",
        "COMPOSER" => "Composer",
        "ORGANIZATION" | "PUBLISHER" | "LABEL" => "Publisher",
        "DESCRIPTION" | "COMMENT" => "Comment",
        "BPM" => "BPM",
        "COPYRIGHT" => "Copyright",
        other => return Some(capitalise(other)),
    };
    Some(friendly.to_string())
}

/// Title-case an unknown Vorbis key (`REPLAYGAIN_TRACK_GAIN` → `Replaygain_track_gain`) for display.
fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars.flat_map(|c| c.to_lowercase())).collect(),
        None => String::new(),
    }
}

// ---- EXIF (CPE-1034) ----

/// Common EXIF tags mapped to a friendly key + editable flag. Camera-set intrinsics (make/model/
/// dimensions/exposure/ISO/focal length/orientation/date/lens) are read-only; the handful of
/// descriptive tags a user legitimately edits are `editable:true`.
const EXIF_TAGS: &[(exif::Tag, &str, bool)] = &[
    (exif::Tag::Make, "Make", false),
    (exif::Tag::Model, "Model", false),
    (exif::Tag::LensModel, "LensModel", false),
    (exif::Tag::DateTimeOriginal, "DateTimeOriginal", false),
    (exif::Tag::ExposureTime, "ExposureTime", false),
    (exif::Tag::FNumber, "FNumber", false),
    (exif::Tag::PhotographicSensitivity, "ISO", false),
    (exif::Tag::FocalLength, "FocalLength", false),
    (exif::Tag::Orientation, "Orientation", false),
    (exif::Tag::PixelXDimension, "PixelXDimension", false),
    (exif::Tag::PixelYDimension, "PixelYDimension", false),
    (exif::Tag::GPSLatitude, "GPS Latitude", false),
    (exif::Tag::GPSLongitude, "GPS Longitude", false),
    (exif::Tag::ImageDescription, "ImageDescription", true),
    (exif::Tag::Artist, "Artist", true),
    (exif::Tag::Copyright, "Copyright", true),
    (exif::Tag::UserComment, "UserComment", true),
];

/// Parse EXIF metadata from an image's leading bytes into [`MetaField`]s in group `"exif"`. Uses the
/// `exif` crate (`kamadak-exif`, already a dependency — do not add another) via
/// `Reader::read_from_container`, which auto-detects TIFF/JPEG/PNG/WebP/HEIF containers and locates the
/// embedded Exif block. On any error — not an image, no Exif data, or truncated/corrupt bytes — this
/// returns an empty vec; it **never panics**.
///
/// Camera intrinsics (Make/Model/dimensions/exposure/ISO/focal length/orientation/date/lens) are
/// `editable:false`; the descriptive tags a user legitimately edits (caption/artist/copyright/comment)
/// are `editable:true`. Values come straight from [`exif::Field::display_value`]`.with_unit(&exif)`, so
/// e.g. ASCII fields render with surrounding quotes and rationals carry their unit (`"f/2.8"`,
/// `"1/200 s"`) — this mirrors the crate's own formatting rather than reinventing it. Fields with an
/// empty display value are skipped.
pub fn read_exif(bytes: &[u8]) -> Vec<MetaField> {
    let Ok(exif) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)) else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    for &(tag, key, editable) in EXIF_TAGS {
        let Some(field) = exif.get_field(tag, exif::In::PRIMARY) else { continue };
        let value = field.display_value().with_unit(&exif).to_string();
        if value.trim().is_empty() {
            continue;
        }
        fields.push(MetaField { group: "exif".to_string(), key: key.to_string(), value, editable });
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a 28-bit value as a 4-byte syncsafe integer (the header/v2.4 frame-size encoding).
    fn syncsafe_bytes(mut v: u32) -> [u8; 4] {
        let mut out = [0u8; 4];
        for i in (0..4).rev() {
            out[i] = (v & 0x7F) as u8;
            v >>= 7;
        }
        out
    }

    /// Build a minimal v2.3 (plain sizes) or v2.4 (syncsafe sizes) tag from `(id, encoding, utf8-text)`
    /// frames. The text is encoded per `encoding` (0 Latin-1, 3 UTF-8; UTF-16 built explicitly in its test).
    fn build_tag(major: u8, frames: &[(&str, u8, &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (id, enc, raw) in frames {
            let mut frame_body = vec![*enc];
            frame_body.extend_from_slice(raw);
            body.extend_from_slice(id.as_bytes()); // 4-char id
            let size = frame_body.len() as u32;
            if major >= 4 {
                body.extend_from_slice(&syncsafe_bytes(size));
            } else {
                body.extend_from_slice(&size.to_be_bytes());
            }
            body.extend_from_slice(&[0, 0]); // frame flags
            body.extend_from_slice(&frame_body);
        }
        let mut tag = Vec::new();
        tag.extend_from_slice(b"ID3");
        tag.extend_from_slice(&[major, 0, 0]); // revision, flags
        tag.extend_from_slice(&syncsafe_bytes(body.len() as u32));
        tag.extend_from_slice(&body);
        tag
    }

    fn get<'a>(fields: &'a [MetaField], key: &str) -> Option<&'a str> {
        fields.iter().find(|f| f.key == key).map(|f| f.value.as_str())
    }

    /// Build a raw Vorbis-comment block from `KEY=VALUE` comments (empty vendor).
    fn build_vorbis(comments: &[&str]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&0u32.to_le_bytes()); // vendor length 0
        b.extend_from_slice(&(comments.len() as u32).to_le_bytes());
        for c in comments {
            b.extend_from_slice(&(c.len() as u32).to_le_bytes());
            b.extend_from_slice(c.as_bytes());
        }
        b
    }

    /// Wrap a Vorbis-comment block in a minimal FLAC stream (magic + a STREAMINFO stub + the comment block).
    fn build_flac(comment_block: &[u8]) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(b"fLaC");
        // Block 0: STREAMINFO (type 0), not last — 4 bytes of filler data.
        f.push(0x00);
        f.extend_from_slice(&[0, 0, 4]);
        f.extend_from_slice(&[0u8; 4]);
        // Block 1: VORBIS_COMMENT (type 4), last block (0x80 | 4).
        f.push(0x84);
        let len = comment_block.len() as u32;
        f.extend_from_slice(&len.to_be_bytes()[1..]); // 3-byte big-endian length
        f.extend_from_slice(comment_block);
        f
    }

    #[test]
    fn parse_vorbis_maps_keys_to_friendly_names() {
        let block = build_vorbis(&["TITLE=Redshift", "ARTIST=Tycho", "TRACKNUMBER=3", "DATE=2011-05-01", "REPLAYGAIN_TRACK_GAIN=-6.5 dB"]);
        let f = parse_vorbis_comment(&block);
        assert_eq!(get(&f, "Title"), Some("Redshift"));
        assert_eq!(get(&f, "Artist"), Some("Tycho"));
        assert_eq!(get(&f, "Track"), Some("3"));
        assert_eq!(get(&f, "Year"), Some("2011-05-01"));
        // Unknown key passes through, capitalised.
        assert_eq!(get(&f, "Replaygain_track_gain"), Some("-6.5 dB"));
        assert!(f.iter().all(|x| x.group == "vorbis" && x.editable));
    }

    #[test]
    fn parse_vorbis_is_case_insensitive_and_skips_blank_and_malformed() {
        let block = build_vorbis(&["title=lower", "ARTIST=", "no-equals-sign", "ALBUM=Dive"]);
        let f = parse_vorbis_comment(&block);
        assert_eq!(get(&f, "Title"), Some("lower")); // case-insensitive key
        assert_eq!(get(&f, "Artist"), None); // blank value dropped
        assert_eq!(get(&f, "Album"), Some("Dive"));
        assert_eq!(f.len(), 2); // the "no-equals-sign" entry is skipped
    }

    #[test]
    fn read_flac_finds_the_comment_block() {
        let flac = build_flac(&build_vorbis(&["TITLE=Awake", "ARTIST=Tycho", "TRACKNUMBER=1/10"]));
        let f = read_flac(&flac);
        assert_eq!(get(&f, "Title"), Some("Awake"));
        assert_eq!(get(&f, "Artist"), Some("Tycho"));
        assert_eq!(get(&f, "Track"), Some("1/10"));
    }

    #[test]
    fn read_flac_rejects_non_flac_and_tolerates_truncation() {
        assert!(read_flac(b"OggS....").is_empty());
        assert!(read_flac(b"fLaC").is_empty()); // magic only, no blocks
        let flac = build_flac(&build_vorbis(&["TITLE=X"]));
        for cut in 4..flac.len() {
            let _ = read_flac(&flac[..cut]); // must never panic on any truncation
        }
    }

    /// Wrap a Vorbis-comment block in a minimal OGG stream: the `OggS` magic + a stub page header + the
    /// `\x03vorbis` comment-header signature + the block + a trailing framing byte.
    fn build_ogg(comment_block: &[u8]) -> Vec<u8> {
        let mut o = Vec::new();
        o.extend_from_slice(b"OggS");
        o.extend_from_slice(&[0u8; 22]); // version + flags + granule + serial + seqno + crc (stubbed)
        o.push(1); // one segment (contents irrelevant to the scan-based reader)
        o.push(0xFF);
        o.extend_from_slice(b"\x03vorbis");
        o.extend_from_slice(comment_block);
        o.push(0x01); // framing bit — parse_vorbis_comment stops before this
        o
    }

    #[test]
    fn read_ogg_extracts_comment_header() {
        let ogg = build_ogg(&build_vorbis(&["TITLE=Weightless", "ARTIST=Marconi Union", "TRACKNUMBER=1"]));
        let f = read_ogg(&ogg);
        assert_eq!(get(&f, "Title"), Some("Weightless"));
        assert_eq!(get(&f, "Artist"), Some("Marconi Union"));
        assert_eq!(get(&f, "Track"), Some("1"));
    }

    #[test]
    fn read_ogg_rejects_non_ogg_and_tolerates_truncation() {
        assert!(read_ogg(b"fLaC....").is_empty());
        assert!(read_ogg(b"OggS-no-vorbis-marker-here").is_empty()); // magic but no comment header
        let ogg = build_ogg(&build_vorbis(&["TITLE=T"]));
        for cut in 4..ogg.len() {
            let _ = read_ogg(&ogg[..cut]); // no panic on any truncation
        }
    }

    #[test]
    fn flac_friendly_keys_line_up_with_id3_for_audio_cell() {
        // Same friendly keys as ID3 → the CPE-971 audio_cell extractor works on FLAC tags unchanged.
        use crate::media_column::{audio_cell, AudioColumn};
        use crate::metadata_column::CellValue;
        let f = read_flac(&build_flac(&build_vorbis(&["TRACKNUMBER=7/12", "DATE=1999"])));
        assert_eq!(audio_cell(&f, AudioColumn::Track), CellValue::Int(7));
        assert_eq!(audio_cell(&f, AudioColumn::Year), CellValue::Int(1999));
    }

    #[test]
    fn non_id3_input_yields_nothing() {
        assert!(read_id3v2(b"").is_empty());
        assert!(read_id3v2(b"not a tag at all").is_empty());
        assert!(read_id3v2(&[0xFF, 0xFB, 0x90, 0x00]).is_empty()); // a bare MP3 frame sync, no ID3
    }

    #[test]
    fn reads_latin1_and_utf8_text_frames_v23() {
        let tag = build_tag(
            3,
            &[
                ("TIT2", 0, b"Bohemian Rhapsody"),   // Latin-1
                ("TPE1", 3, "Queen".as_bytes()),     // UTF-8
                ("TALB", 0, b"A Night at the Opera"),
                ("TRCK", 0, b"11/12"),
            ],
        );
        let f = read_id3v2(&tag);
        assert_eq!(get(&f, "Title"), Some("Bohemian Rhapsody"));
        assert_eq!(get(&f, "Artist"), Some("Queen"));
        assert_eq!(get(&f, "Album"), Some("A Night at the Opera"));
        assert_eq!(get(&f, "Track"), Some("11/12"));
        assert!(f.iter().all(|x| x.group == "id3" && x.editable));
    }

    #[test]
    fn reads_utf8_and_utf16_and_v24_syncsafe_sizes() {
        // UTF-8 non-ASCII + a UTF-16 (LE BOM) frame, under v2.4 syncsafe frame sizes.
        let utf16_le = {
            let mut v = vec![0xFF, 0xFE];
            for u in "Étude".encode_utf16() {
                v.extend_from_slice(&u.to_le_bytes());
            }
            v
        };
        let tag = build_tag(
            4,
            &[
                ("TPE1", 3, "Sigur Rós".as_bytes()), // UTF-8
                ("TIT2", 1, &utf16_le),              // UTF-16 with BOM
            ],
        );
        let f = read_id3v2(&tag);
        assert_eq!(get(&f, "Artist"), Some("Sigur Rós"));
        assert_eq!(get(&f, "Title"), Some("Étude"));
    }

    #[test]
    fn maps_genre_year_composer_and_passes_unknown_text_frames_through() {
        let tag = build_tag(
            3,
            &[
                ("TCON", 0, b"Rock"),
                ("TYER", 0, b"1975"),
                ("TCOM", 0, b"Freddie Mercury"),
                ("TSSE", 0, b"LAME 3.100"), // unknown text frame → passes through under its id
            ],
        );
        let f = read_id3v2(&tag);
        assert_eq!(get(&f, "Genre"), Some("Rock"));
        assert_eq!(get(&f, "Year"), Some("1975"));
        assert_eq!(get(&f, "Composer"), Some("Freddie Mercury"));
        assert_eq!(get(&f, "TSSE"), Some("LAME 3.100"));
    }

    #[test]
    fn decodes_comment_frames_skipping_language_and_description() {
        // COMM body: enc(0) + lang("eng") + desc("")\0 + text.
        let mut comm = vec![0u8];
        comm.extend_from_slice(b"eng");
        comm.push(0); // empty description terminator
        comm.extend_from_slice(b"Remastered 2011");
        let tag = build_tag(3, &[("COMM", 0, &comm[1..])]); // build_tag re-prepends the encoding byte
        let f = read_id3v2(&tag);
        assert_eq!(get(&f, "Comment"), Some("Remastered 2011"));
    }

    #[test]
    fn empty_text_frames_are_dropped() {
        let tag = build_tag(3, &[("TIT2", 0, b""), ("TPE1", 0, b"Someone")]);
        let f = read_id3v2(&tag);
        assert_eq!(get(&f, "Title"), None); // blank title not surfaced
        assert_eq!(get(&f, "Artist"), Some("Someone"));
    }

    #[test]
    fn truncated_tag_does_not_panic_and_returns_partial() {
        let full = build_tag(3, &[("TIT2", 0, b"Complete Title"), ("TPE1", 0, b"Complete Artist")]);
        // Chop the buffer mid-second-frame: the first frame still parses, the rest is ignored (no panic).
        for cut in 10..full.len() {
            let partial = &full[..cut];
            let _ = read_id3v2(partial); // must never panic
        }
        // A cut that keeps the whole first frame still yields the title.
        let f = read_id3v2(&full[..full.len() - 3]);
        assert_eq!(get(&f, "Title"), Some("Complete Title"));
    }

    #[test]
    fn stops_cleanly_on_padding_and_zero_sized_frames() {
        let mut tag = build_tag(3, &[("TIT2", 0, b"Only One")]);
        tag.extend_from_slice(&[0u8; 20]); // trailing padding (NUL frame ids)
        let f = read_id3v2(&tag);
        assert_eq!(f.len(), 1);
        assert_eq!(get(&f, "Title"), Some("Only One"));
    }

    #[test]
    fn reads_v22_three_char_frames() {
        // v2.2: 3-char ids, 3-byte big-endian sizes, no frame flags.
        let mut body = Vec::new();
        for (id, text) in [("TT2", &b"Yesterday"[..]), ("TP1", &b"The Beatles"[..])] {
            let mut fb = vec![0u8]; // Latin-1 encoding byte
            fb.extend_from_slice(text);
            body.extend_from_slice(id.as_bytes());
            let sz = fb.len() as u32;
            body.extend_from_slice(&sz.to_be_bytes()[1..]); // low 3 bytes
            body.extend_from_slice(&fb);
        }
        let mut tag = Vec::new();
        tag.extend_from_slice(b"ID3");
        tag.extend_from_slice(&[2, 0, 0]);
        tag.extend_from_slice(&syncsafe_bytes(body.len() as u32));
        tag.extend_from_slice(&body);
        let f = read_id3v2(&tag);
        assert_eq!(get(&f, "Title"), Some("Yesterday"));
        assert_eq!(get(&f, "Artist"), Some("The Beatles"));
    }

    // ---- EXIF fixture builder (CPE-1034) ----
    //
    // A minimal hand-built little-endian TIFF/Exif container: an 8-byte TIFF header + IFD0 (image-level
    // tags, plus pointers to the Exif and GPS sub-IFDs when needed) + the Exif sub-IFD + the GPS
    // sub-IFD. `exif::Reader::read_from_container` recognises raw TIFF bytes directly via the `II*\0`
    // signature, so no JPEG/PNG wrapping is needed for a fixture — this *is* what a `.tif`/`.tiff` file's
    // header looks like, and it's also byte-for-byte what a JPEG's embedded Exif APP1 payload contains.

    /// One raw IFD entry: tag, TIFF type code, count, and its type-specific value bytes (already
    /// little-endian encoded). Written inline in the entry's 4-byte value slot when `data.len() <= 4`
    /// (per the TIFF spec); otherwise placed in this IFD's own overflow area right after its entries.
    struct RawEntry { tag: u16, typ: u16, count: u32, data: Vec<u8> }

    const T_ASCII: u16 = 2;
    const T_SHORT: u16 = 3;
    const T_LONG: u16 = 4;
    const T_RATIONAL: u16 = 5;

    fn ascii_entry(tag: u16, s: &str) -> RawEntry {
        let mut data = s.as_bytes().to_vec();
        data.push(0); // NUL-terminated, per TIFF ASCII fields
        RawEntry { tag, typ: T_ASCII, count: data.len() as u32, data }
    }
    fn short_entry(tag: u16, v: u16) -> RawEntry {
        RawEntry { tag, typ: T_SHORT, count: 1, data: v.to_le_bytes().to_vec() }
    }
    fn long_entry(tag: u16, v: u32) -> RawEntry {
        RawEntry { tag, typ: T_LONG, count: 1, data: v.to_le_bytes().to_vec() }
    }
    fn rational_entry(tag: u16, num: u32, den: u32) -> RawEntry {
        let mut data = num.to_le_bytes().to_vec();
        data.extend_from_slice(&den.to_le_bytes());
        RawEntry { tag, typ: T_RATIONAL, count: 1, data }
    }
    fn rational3_entry(tag: u16, vals: [(u32, u32); 3]) -> RawEntry {
        let mut data = Vec::new();
        for (n, d) in vals {
            data.extend_from_slice(&n.to_le_bytes());
            data.extend_from_slice(&d.to_le_bytes());
        }
        RawEntry { tag, typ: T_RATIONAL, count: 3, data }
    }

    /// Total bytes an IFD's header (count + 12-byte entries + next-IFD offset) plus its own overflow
    /// data will occupy — used to compute where the *next* IFD in the file must start.
    fn ifd_block_len(entries: &[RawEntry]) -> u32 {
        let header = 2 + entries.len() as u32 * 12 + 4;
        let overflow: u32 = entries.iter().map(|e| if e.data.len() > 4 { e.data.len() as u32 } else { 0 }).sum();
        header + overflow
    }

    /// Serialize one IFD at the absolute file offset `start`, given the offset of the next IFD (0 if
    /// none). Entries whose value doesn't fit inline get their overflow bytes appended right after this
    /// IFD's own entry table, with the entry pointing at that offset — mirroring how a real TIFF writer
    /// lays out variable-length field data.
    fn build_ifd(start: u32, entries: &[RawEntry], next_ifd: u32) -> Vec<u8> {
        let header_len = 2 + entries.len() as u32 * 12 + 4;
        let mut overflow_offset = start + header_len;
        let mut out = Vec::new();
        let mut overflow = Vec::new();
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for e in entries {
            out.extend_from_slice(&e.tag.to_le_bytes());
            out.extend_from_slice(&e.typ.to_le_bytes());
            out.extend_from_slice(&e.count.to_le_bytes());
            if e.data.len() <= 4 {
                let mut inline = [0u8; 4];
                inline[..e.data.len()].copy_from_slice(&e.data);
                out.extend_from_slice(&inline);
            } else {
                out.extend_from_slice(&overflow_offset.to_le_bytes());
                overflow.extend_from_slice(&e.data);
                overflow_offset += e.data.len() as u32;
            }
        }
        out.extend_from_slice(&next_ifd.to_le_bytes());
        out.extend_from_slice(&overflow);
        out
    }

    /// Build a full little-endian TIFF/Exif fixture from IFD0's own entries plus optional Exif-sub-IFD
    /// and GPS-sub-IFD entries (each skipped, and its pointer omitted from IFD0, when empty).
    fn build_exif_tiff(mut ifd0: Vec<RawEntry>, exif_ifd: Vec<RawEntry>, gps_ifd: Vec<RawEntry>) -> Vec<u8> {
        const EXIF_IFD_POINTER: u16 = 0x8769;
        const GPS_INFO_IFD_POINTER: u16 = 0x8825;
        if !exif_ifd.is_empty() {
            ifd0.push(long_entry(EXIF_IFD_POINTER, 0)); // placeholder, patched below
        }
        if !gps_ifd.is_empty() {
            ifd0.push(long_entry(GPS_INFO_IFD_POINTER, 0));
        }
        const HEADER_LEN: u32 = 8;
        let exif_offset = HEADER_LEN + ifd_block_len(&ifd0);
        let gps_offset = exif_offset + ifd_block_len(&exif_ifd);

        // Now that the sub-IFD offsets are known, patch the pointer entries with their real values.
        for e in ifd0.iter_mut() {
            if e.tag == EXIF_IFD_POINTER {
                e.data = exif_offset.to_le_bytes().to_vec();
            } else if e.tag == GPS_INFO_IFD_POINTER {
                e.data = gps_offset.to_le_bytes().to_vec();
            }
        }

        let mut buf = Vec::new();
        buf.extend_from_slice(b"II*\0"); // little-endian TIFF signature
        buf.extend_from_slice(&HEADER_LEN.to_le_bytes()); // offset of IFD0
        buf.extend_from_slice(&build_ifd(HEADER_LEN, &ifd0, 0));
        if !exif_ifd.is_empty() {
            buf.extend_from_slice(&build_ifd(exif_offset, &exif_ifd, 0));
        }
        if !gps_ifd.is_empty() {
            buf.extend_from_slice(&build_ifd(gps_offset, &gps_ifd, 0));
        }
        buf
    }

    /// A fixture exercising the camera-intrinsic tags (IFD0 + Exif sub-IFD + GPS sub-IFD) plus one
    /// descriptive (editable) tag, `ImageDescription`.
    fn full_exif_fixture() -> Vec<u8> {
        let ifd0 = vec![
            ascii_entry(0x010E, "Sunset"),        // ImageDescription
            ascii_entry(0x010F, "Acme"),          // Make
            ascii_entry(0x0110, "Zenith 3000"),   // Model
            short_entry(0x0112, 1),               // Orientation
        ];
        let exif_ifd = vec![
            rational_entry(0x829A, 1, 200),               // ExposureTime = 1/200 s
            rational_entry(0x829D, 28, 10),               // FNumber = f/2.8
            short_entry(0x8827, 400),                     // PhotographicSensitivity (ISO)
            ascii_entry(0x9003, "2024:01:02 03:04:05"),   // DateTimeOriginal
            rational_entry(0x920A, 50, 1),                // FocalLength = 50 mm
            long_entry(0xA002, 4000),                     // PixelXDimension
            long_entry(0xA003, 3000),                     // PixelYDimension
        ];
        let gps_ifd = vec![
            ascii_entry(0x0001, "N"),                                  // GPSLatitudeRef
            rational3_entry(0x0002, [(37, 1), (46, 1), (30, 1)]),      // GPSLatitude
            ascii_entry(0x0003, "W"),                                  // GPSLongitudeRef
            rational3_entry(0x0004, [(122, 1), (25, 1), (9, 1)]),      // GPSLongitude
        ];
        build_exif_tiff(ifd0, exif_ifd, gps_ifd)
    }

    #[test]
    fn read_exif_maps_common_fields_with_editable_flags() {
        let f = read_exif(&full_exif_fixture());

        // ASCII fields render quoted (the crate's own Display impl for Value::Ascii); numeric/DMS/
        // datetime fields don't. This test asserts exactly what `display_value().with_unit(&exif)`
        // produces for our fixture, so it locks in real parsing rather than a hand-guessed string.
        assert_eq!(get(&f, "Make"), Some("\"Acme\""));
        assert_eq!(get(&f, "Model"), Some("\"Zenith 3000\""));
        assert_eq!(get(&f, "ImageDescription"), Some("\"Sunset\""));
        assert_eq!(get(&f, "Orientation"), Some("row 0 at top and column 0 at left"));
        assert_eq!(get(&f, "DateTimeOriginal"), Some("2024-01-02 03:04:05"));
        assert_eq!(get(&f, "PixelXDimension"), Some("4000 pixels"));
        assert_eq!(get(&f, "PixelYDimension"), Some("3000 pixels"));
        assert_eq!(get(&f, "ISO"), Some("400"));
        assert_eq!(get(&f, "ExposureTime"), Some("1/200 s"));
        assert_eq!(get(&f, "FNumber"), Some("f/2.8"));
        assert_eq!(get(&f, "FocalLength"), Some("50 mm"));
        assert_eq!(get(&f, "GPS Latitude"), Some("37 deg 46 min 30 sec N"));
        assert_eq!(get(&f, "GPS Longitude"), Some("122 deg 25 min 9 sec W"));

        // Camera intrinsics are read-only, and every field lands in the "exif" group.
        for key in [
            "Make", "Model", "Orientation", "DateTimeOriginal", "PixelXDimension", "PixelYDimension",
            "ISO", "ExposureTime", "FNumber", "FocalLength", "GPS Latitude", "GPS Longitude",
        ] {
            let field = f.iter().find(|x| x.key == key).unwrap_or_else(|| panic!("missing {key}"));
            assert_eq!(field.group, "exif");
            assert!(!field.editable, "{key} should be read-only");
        }
        // The one descriptive tag in this fixture is user-editable.
        let desc = f.iter().find(|x| x.key == "ImageDescription").unwrap();
        assert_eq!(desc.group, "exif");
        assert!(desc.editable);
    }

    #[test]
    fn read_exif_maps_descriptive_tags_as_editable() {
        let ifd0 = vec![
            ascii_entry(0x013B, "Jane Doe"),           // Artist
            ascii_entry(0x8298, "(c) 2024 Jane Doe"),  // Copyright
        ];
        let exif_ifd = vec![
            ascii_entry(0xA434, "50mm Prime"), // LensModel
            ascii_entry(0x9286, "Shot at dawn"), // UserComment
        ];
        let bytes = build_exif_tiff(ifd0, exif_ifd, Vec::new());
        let f = read_exif(&bytes);

        // The user-editable descriptive tags.
        for (key, expected) in [
            ("Artist", "\"Jane Doe\""),
            ("Copyright", "\"(c) 2024 Jane Doe\""),
            ("UserComment", "\"Shot at dawn\""),
        ] {
            let field = f.iter().find(|x| x.key == key).unwrap_or_else(|| panic!("missing {key}"));
            assert_eq!(field.value, expected);
            assert_eq!(field.group, "exif");
            assert!(field.editable, "{key} should be editable");
        }
        // LensModel is a camera-set intrinsic like Make/Model, not a user-editable descriptive tag.
        let lens = f.iter().find(|x| x.key == "LensModel").unwrap();
        assert_eq!(lens.value, "\"50mm Prime\"");
        assert!(!lens.editable, "LensModel should be read-only");
    }

    #[test]
    fn read_exif_skips_absent_gps_and_descriptive_tags() {
        // Only Make/Model — no GPS sub-IFD, no ImageDescription/Artist/etc.
        let ifd0 = vec![ascii_entry(0x010F, "Acme"), ascii_entry(0x0110, "Zenith 3000")];
        let bytes = build_exif_tiff(ifd0, Vec::new(), Vec::new());
        let f = read_exif(&bytes);

        assert_eq!(get(&f, "Make"), Some("\"Acme\""));
        assert_eq!(get(&f, "Model"), Some("\"Zenith 3000\""));
        assert_eq!(f.iter().find(|x| x.key == "GPS Latitude"), None);
        assert_eq!(f.iter().find(|x| x.key == "ImageDescription"), None);
        assert_eq!(f.iter().find(|x| x.key == "ExposureTime"), None);
        assert_eq!(f.len(), 2); // nothing else got surfaced
    }

    #[test]
    fn read_exif_returns_empty_for_non_image_bytes() {
        assert!(read_exif(b"").is_empty());
        assert!(read_exif(b"not an image at all, just plain text bytes").is_empty());
        assert!(read_exif(b"ID3\x03\0\0\0\0\0\0").is_empty()); // a real ID3 tag, but not an image
    }

    #[test]
    fn read_exif_tolerates_truncation_without_panicking() {
        let bytes = full_exif_fixture();
        for cut in 0..bytes.len() {
            let _ = read_exif(&bytes[..cut]); // must never panic on any truncation
        }
        // A cut that keeps only the TIFF header (no IFD body at all) yields nothing.
        assert!(read_exif(&bytes[..8]).is_empty());
    }
}
