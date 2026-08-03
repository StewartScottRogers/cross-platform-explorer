//! Media-metadata **write codecs** (CPE-1035 ID3v2, CPE-1038 FLAC/Vorbis; epic CPE-725): the counterpart
//! to [`crate::media_meta_read`]'s read side. [`crate::media_meta_edit`] owns the edit *policy* over
//! [`MetaField`]s but does no file parsing/writing — "the codec layer reads the fields in and writes the
//! result back." [`crate::media_meta_read`] is that read layer; this module is the write layer:
//! - [`write_id3v2`] builds a fresh, valid **ID3v2.4** tag from the `group == "id3"` fields and prepends it
//!   to the original audio payload, replacing any pre-existing tag rather than stacking on top of it.
//! - [`write_flac`] rebuilds a **FLAC** stream with its `VORBIS_COMMENT` metadata block replaced by one
//!   built from the `group == "vorbis"` fields (via [`write_vorbis_comment`]), preserving STREAMINFO,
//!   every other metadata block, and the audio frames byte-for-byte.
//!
//! Pure + std-only (no new deps): fully cargo-testable by round-tripping through
//! [`crate::media_meta_read`]'s readers with synthesised inputs, and does no I/O. Bounds-checked
//! throughout — malformed/short/empty input never panics; at worst the whole input is treated as the
//! audio payload with no pre-existing tag to strip (ID3), or is returned unchanged (FLAC).

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

// ---- FLAC / Vorbis comment write-back (CPE-1038) ----

/// Build a raw Vorbis-comment block from the `group == "vorbis"` fields in `fields` — the inverse of
/// [`crate::media_meta_read::parse_vorbis_comment`]. Layout (all lengths little-endian u32): vendor length,
/// vendor string, comment count, then for each entry: length, then `KEY=value` (UTF-8). Each friendly key
/// is mapped back to its uppercase Vorbis key via [`vorbis_field_key`]; fields with no known mapping, or a
/// blank value, are skipped (mirrors [`build_frames`]'s handling of the `"id3"` group).
pub fn write_vorbis_comment(fields: &[MetaField]) -> Vec<u8> {
    const VENDOR: &[u8] = b"cpe-server";

    let mut entries: Vec<Vec<u8>> = Vec::new();
    for f in fields {
        if f.group != "vorbis" || f.value.is_empty() {
            continue;
        }
        let Some(key) = vorbis_field_key(&f.key) else { continue };
        let mut entry = Vec::with_capacity(key.len() + 1 + f.value.len());
        entry.extend_from_slice(key.as_bytes());
        entry.push(b'=');
        entry.extend_from_slice(f.value.as_bytes());
        entries.push(entry);
    }

    let mut out = Vec::with_capacity(4 + VENDOR.len() + 4 + entries.iter().map(|e| 4 + e.len()).sum::<usize>());
    out.extend_from_slice(&(VENDOR.len() as u32).to_le_bytes());
    out.extend_from_slice(VENDOR);
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for e in &entries {
        out.extend_from_slice(&(e.len() as u32).to_le_bytes());
        out.extend_from_slice(e);
    }
    out
}

/// Map a friendly key (as produced by [`crate::media_meta_read`]'s private `vorbis_key`) back to its
/// uppercase raw Vorbis-comment field name. Where the reader folds several raw names onto one friendly key
/// (e.g. `ORGANIZATION`/`PUBLISHER`/`LABEL` → `Publisher`), this picks the standard Vorbis field name. A
/// friendly key that isn't one of the known names is passed through uppercased when it's already
/// plausible as a raw key (letters/digits/underscore) — this inverts the reader's `capitalise` fallback
/// for an unrecognised comment (e.g. `Replaygain_track_gain` → `REPLAYGAIN_TRACK_GAIN`) so those
/// round-trip too. Anything else (e.g. a friendly key with spaces, like `"Album Artist"`, is handled by
/// the explicit match above it) that still doesn't look like a raw key is skipped.
fn vorbis_field_key(friendly: &str) -> Option<String> {
    let key = match friendly {
        "Title" => "TITLE",
        "Artist" => "ARTIST",
        "Album" => "ALBUM",
        "Album Artist" => "ALBUMARTIST",
        "Track" => "TRACKNUMBER",
        "Disc" => "DISCNUMBER",
        "Genre" => "GENRE",
        "Year" => "DATE",
        "Composer" => "COMPOSER",
        "Publisher" => "ORGANIZATION",
        "Comment" => "COMMENT",
        "BPM" => "BPM",
        "Copyright" => "COPYRIGHT",
        other => {
            let upper = other.to_ascii_uppercase();
            let is_plausible_raw_key = !upper.is_empty() && upper.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            return if is_plausible_raw_key { Some(upper) } else { None };
        }
    };
    Some(key.to_string())
}

/// One parsed FLAC metadata block: its raw type (bits 0-6 of the header byte) and the byte range of its
/// *data* (header excluded) within the original buffer.
struct FlacBlock {
    block_type: u8,
    data_start: usize,
    data_end: usize,
}

/// Parse the FLAC metadata-block chain starting right after the `fLaC` magic (offset 4). Returns the
/// parsed blocks plus the offset where the audio frames begin (i.e. right after the last metadata block),
/// or `None` if the chain is malformed/truncated in any way — bounds-checked throughout, never panics.
fn parse_flac_blocks(bytes: &[u8]) -> Option<(Vec<FlacBlock>, usize)> {
    let mut blocks = Vec::new();
    let mut pos = 4usize;
    loop {
        if pos + 4 > bytes.len() {
            return None; // truncated header
        }
        let header = bytes[pos];
        let is_last = header & 0x80 != 0;
        let block_type = header & 0x7F;
        let len = be_u24(&bytes[pos + 1..pos + 4]) as usize;
        let data_start = pos + 4;
        let data_end = data_start + len;
        if data_end > bytes.len() {
            return None; // truncated block data
        }
        blocks.push(FlacBlock { block_type, data_start, data_end });
        pos = data_end;
        if is_last {
            break;
        }
    }
    if blocks.is_empty() {
        return None;
    }
    Some((blocks, pos))
}

/// Read a big-endian u24 from 3 bytes (mirrors `media_meta_read`'s private helper; re-implemented locally
/// for the same reason as [`syncsafe28_decode`]).
fn be_u24(b: &[u8]) -> u32 {
    ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32)
}

/// Rebuild a FLAC stream (`orig`) with its `VORBIS_COMMENT` metadata block (type 4) replaced by a fresh one
/// built from the `"vorbis"`-group fields in `fields` (via [`write_vorbis_comment`]) — inserted right after
/// STREAMINFO if none existed yet. STREAMINFO (type 0, kept first) and every other metadata block are
/// preserved byte-for-byte, as are the audio frames that follow the metadata chain; only the
/// last-metadata-block flag is recomputed so it lands on the true final block. If `orig` isn't a
/// well-formed FLAC stream (missing magic, truncated/garbage block chain), it is returned unchanged —
/// never panics.
pub fn write_flac(orig: &[u8], fields: &[MetaField]) -> Vec<u8> {
    if orig.len() < 4 || &orig[0..4] != b"fLaC" {
        return orig.to_vec();
    }
    let Some((blocks, audio_start)) = parse_flac_blocks(orig) else {
        return orig.to_vec();
    };
    let audio = &orig[audio_start..];
    let comment_data = write_vorbis_comment(fields);

    // Keep every block's raw data as-is except the VORBIS_COMMENT one, which is replaced.
    let mut new_blocks: Vec<(u8, &[u8])> = Vec::with_capacity(blocks.len() + 1);
    let mut found_comment = false;
    for b in &blocks {
        if b.block_type == 4 {
            new_blocks.push((4, &comment_data[..]));
            found_comment = true;
        } else {
            new_blocks.push((b.block_type, &orig[b.data_start..b.data_end]));
        }
    }
    if !found_comment {
        // STREAMINFO (type 0) must be first per the FLAC spec; insert the new comment block right after
        // it. If the first block somehow isn't STREAMINFO (malformed input we still parsed), fall back to
        // inserting at the very front rather than guessing further.
        let insert_at = if new_blocks.first().map(|(t, _)| *t) == Some(0) { 1 } else { 0 };
        new_blocks.insert(insert_at, (4, &comment_data[..]));
    }

    let total_len: usize = 4 + new_blocks.iter().map(|(_, data)| 4 + data.len()).sum::<usize>() + audio.len();
    let mut out = Vec::with_capacity(total_len);
    out.extend_from_slice(b"fLaC");
    let last_index = new_blocks.len() - 1;
    for (i, (block_type, data)) in new_blocks.iter().enumerate() {
        let mut header = block_type & 0x7F;
        if i == last_index {
            header |= 0x80;
        }
        out.push(header);
        let len_bytes = (data.len() as u32).to_be_bytes();
        out.extend_from_slice(&len_bytes[1..4]); // 3-byte big-endian length
        out.extend_from_slice(data);
    }
    out.extend_from_slice(audio);
    out
}

// ---- EXIF / JPEG write-back (CPE-1288) ----

/// Rebuild a JPEG (`orig`) with the four editable EXIF tags — ImageDescription / Artist / Copyright /
/// UserComment (the ones [`crate::media_meta_read::read_exif`] marks `editable`) — taken from the
/// `group == "exif"` fields in `fields`, encoded into a fresh Exif APP1 segment that **replaces** any
/// existing Exif APP1 while preserving every other segment and the entropy-coded scan data byte-for-byte.
///
/// The new APP1 (`FFE1` + 2-byte big-endian length + `"Exif\0\0"` + a TIFF/Exif block built with the
/// vendored `kamadak-exif` crate's experimental `Writer`) is inserted right after the SOI (`FFD8`) and any
/// leading APP0 JFIF segment, matching where cameras place it; a pre-existing `Exif\0\0` APP1 anywhere in
/// the header is stripped so re-writing is idempotent rather than stacking.
///
/// Read values are un-quoted first (the reader renders ASCII tags with surrounding quotes via
/// `display_value`), so a field carried through unchanged round-trips; UserComment is written with the
/// standard 8-byte `ASCII\0\0\0` character-code prefix, or, if the value is still the reader's `0x…` hex
/// form, decoded back to its raw bytes so it round-trips too. Camera intrinsics (Make/Model/exposure/…)
/// are never rebuilt — only the four editable tags are — so this is not a lossy re-encode of the whole IFD.
///
/// Returns `Err` (never panics) for non-JPEG/truncated input, when there are no editable EXIF fields to
/// write, or when the encoded Exif block won't fit a single 64 KiB APP1 segment. **JPEG only** — for a
/// TIFF the EXIF *is* the file's own IFD chain (rewriting it would drop the image strips/thumbnail), so
/// `tif`/`tiff` are deliberately left out of [`crate::media_meta::is_writable`] for now.
pub fn write_exif(orig: &[u8], fields: &[MetaField]) -> Result<Vec<u8>, String> {
    if orig.len() < 2 || orig[0] != 0xFF || orig[1] != 0xD8 {
        return Err("not a JPEG (missing SOI marker)".into());
    }
    let tiff = build_exif_tiff(fields)?;

    // APP1 segment: FFE1 + 2-byte length (covers the length field itself + payload) + "Exif\0\0" + TIFF.
    let payload_len = 6 + tiff.len();
    let seg_len = payload_len + 2;
    if seg_len > 0xFFFF {
        return Err("EXIF data too large for a JPEG APP1 segment".into());
    }
    let mut app1 = Vec::with_capacity(4 + payload_len);
    app1.extend_from_slice(&[0xFF, 0xE1]);
    app1.extend_from_slice(&(seg_len as u16).to_be_bytes());
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(&tiff);

    let mut out = Vec::with_capacity(orig.len() + app1.len());
    out.extend_from_slice(&orig[0..2]); // SOI
    let mut pos = 2usize;
    let mut inserted = false;
    loop {
        // Need at least a 2-byte marker to continue parsing the header.
        if pos + 1 >= orig.len() {
            if !inserted {
                out.extend_from_slice(&app1);
                inserted = true;
            }
            out.extend_from_slice(&orig[pos..]);
            break;
        }
        if orig[pos] != 0xFF {
            // Out of sync (or scan data we didn't parse into) — copy the remainder verbatim.
            if !inserted {
                out.extend_from_slice(&app1);
                inserted = true;
            }
            out.extend_from_slice(&orig[pos..]);
            break;
        }
        let marker = orig[pos + 1];
        // A run of 0xFF fill bytes may precede a real marker; emit one and re-align.
        if marker == 0xFF {
            out.push(0xFF);
            pos += 1;
            continue;
        }
        // Stand-alone markers carry no length field: TEM (0x01), RSTn/SOI/EOI (0xD0..=0xD9).
        if marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
            if !inserted {
                out.extend_from_slice(&app1);
                inserted = true;
            }
            out.extend_from_slice(&orig[pos..pos + 2]);
            pos += 2;
            if marker == 0xD9 {
                // EOI — copy any trailer and stop.
                out.extend_from_slice(&orig[pos..]);
                break;
            }
            continue;
        }
        // Every other marker has a 2-byte big-endian length (which includes those 2 bytes).
        if pos + 4 > orig.len() {
            if !inserted {
                out.extend_from_slice(&app1);
                inserted = true;
            }
            out.extend_from_slice(&orig[pos..]);
            break;
        }
        let seg_len_here = u16::from_be_bytes([orig[pos + 2], orig[pos + 3]]) as usize;
        if seg_len_here < 2 {
            return Err("malformed JPEG (segment length < 2)".into());
        }
        let seg_end = pos + 2 + seg_len_here;
        if seg_end > orig.len() {
            if !inserted {
                out.extend_from_slice(&app1);
                inserted = true;
            }
            out.extend_from_slice(&orig[pos..]);
            break;
        }
        // SOS (0xDA): the scan data begins after it. Insert our APP1 (if not yet placed) then copy the
        // SOS header and everything after it byte-for-byte.
        if marker == 0xDA {
            if !inserted {
                out.extend_from_slice(&app1);
                inserted = true;
            }
            out.extend_from_slice(&orig[pos..]);
            break;
        }
        let payload = &orig[pos + 4..seg_end];
        // Keep any leading APP0 (JFIF) ahead of our APP1, matching conventional segment order.
        if marker == 0xE0 && !inserted {
            out.extend_from_slice(&orig[pos..seg_end]);
            pos = seg_end;
            continue;
        }
        // First non-APP0 header segment: this is where the new Exif APP1 goes.
        if !inserted {
            out.extend_from_slice(&app1);
            inserted = true;
        }
        // Strip a pre-existing Exif APP1 (replace, don't stack); keep every other segment verbatim.
        if marker == 0xE1 && payload.starts_with(b"Exif\0\0") {
            pos = seg_end;
            continue;
        }
        out.extend_from_slice(&orig[pos..seg_end]);
        pos = seg_end;
    }
    if !inserted {
        out.extend_from_slice(&app1);
    }
    Ok(out)
}

/// Build a raw TIFF/Exif block (big-endian) carrying only the editable tags found in `fields`, via
/// `kamadak-exif`'s experimental `Writer`. `Err` if no writable field is present (an Exif block must have
/// at least one field) or the encoder fails.
fn build_exif_tiff(fields: &[MetaField]) -> Result<Vec<u8>, String> {
    let exif_fields = build_exif_fields(fields);
    if exif_fields.is_empty() {
        return Err("no editable EXIF fields to write".into());
    }
    let mut writer = exif::experimental::Writer::new();
    for f in &exif_fields {
        writer.push_field(f);
    }
    let mut cursor = std::io::Cursor::new(Vec::new());
    writer.write(&mut cursor, false).map_err(|e| format!("failed to encode EXIF: {e}"))?;
    Ok(cursor.into_inner())
}

/// Translate the `"exif"`-group, editable fields into `exif::Field`s the `Writer` understands. Only the
/// four editable tags are emitted; blank values, other groups, and camera-intrinsic keys are skipped
/// (mirrors [`build_frames`]'s handling of the `"id3"` group). ASCII tags are un-quoted; UserComment gets
/// its standard character-code prefix (see [`user_comment_bytes`]).
fn build_exif_fields(fields: &[MetaField]) -> Vec<exif::Field> {
    let mut out = Vec::new();
    for f in fields {
        if !f.group.eq_ignore_ascii_case("exif") || f.value.is_empty() {
            continue;
        }
        let field = match f.key.to_ascii_lowercase().as_str() {
            "imagedescription" => exif::Field {
                tag: exif::Tag::ImageDescription,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Ascii(vec![unquote_display_ascii(&f.value).into_bytes()]),
            },
            "artist" => exif::Field {
                tag: exif::Tag::Artist,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Ascii(vec![unquote_display_ascii(&f.value).into_bytes()]),
            },
            "copyright" => exif::Field {
                tag: exif::Tag::Copyright,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Ascii(vec![unquote_display_ascii(&f.value).into_bytes()]),
            },
            "usercomment" => exif::Field {
                tag: exif::Tag::UserComment,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Undefined(user_comment_bytes(&f.value), 0),
            },
            _ => continue,
        };
        out.push(field);
    }
    out
}

/// Undo [`crate::media_meta_read::read_exif`]'s ASCII display formatting (`display_value` wraps ASCII in
/// double quotes and escapes `\"`, `\\`, and non-printable bytes as `\xNN`) so a value read from the file
/// and carried through unchanged is written back as its original text. A value that isn't wrapped in
/// quotes (e.g. a freshly-typed edit) is returned as-is.
fn unquote_display_ascii(value: &str) -> String {
    let b = value.as_bytes();
    if b.len() < 2 || b[0] != b'"' || b[b.len() - 1] != b'"' {
        return value.to_string();
    }
    let inner = &b[1..b.len() - 1];
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        if inner[i] == b'\\' && i + 1 < inner.len() {
            match inner[i + 1] {
                b'"' => {
                    out.push(b'"');
                    i += 2;
                }
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                }
                b'x' if i + 3 < inner.len() => {
                    let hi = (inner[i + 2] as char).to_digit(16);
                    let lo = (inner[i + 3] as char).to_digit(16);
                    if let (Some(hi), Some(lo)) = (hi, lo) {
                        out.push((hi * 16 + lo) as u8);
                        i += 4;
                    } else {
                        out.push(inner[i]);
                        i += 1;
                    }
                }
                _ => {
                    out.push(inner[i]);
                    i += 1;
                }
            }
        } else {
            out.push(inner[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Encode a UserComment tag body. EXIF stores UserComment as an UNDEFINED value whose first 8 bytes are a
/// character-code (`ASCII\0\0\0`, `UNICODE\0`, …). A freshly-typed edit is written as ASCII with that
/// prefix; a value still in the reader's `0x…` hex form (UNDEFINED renders as hex) is decoded back to its
/// exact raw bytes so a carried-through comment round-trips unchanged.
fn user_comment_bytes(value: &str) -> Vec<u8> {
    if let Some(hex) = value.strip_prefix("0x") {
        if !hex.is_empty() && hex.len() % 2 == 0 && hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            let hb = hex.as_bytes();
            let mut raw = Vec::with_capacity(hb.len() / 2);
            let mut i = 0;
            while i + 1 < hb.len() {
                let hi = (hb[i] as char).to_digit(16).unwrap_or(0);
                let lo = (hb[i + 1] as char).to_digit(16).unwrap_or(0);
                raw.push((hi * 16 + lo) as u8);
                i += 2;
            }
            return raw;
        }
    }
    let mut out = Vec::with_capacity(8 + value.len());
    out.extend_from_slice(b"ASCII\0\0\0");
    out.extend_from_slice(value.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_meta_read::{parse_vorbis_comment, read_flac, read_id3v2};

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

    // ---- FLAC / Vorbis-comment write-back (CPE-1038) ----

    fn vfield(key: &str, value: &str) -> MetaField {
        MetaField { group: "vorbis".to_string(), key: key.to_string(), value: value.to_string(), editable: true }
    }

    /// Serialize one raw FLAC metadata block: 1-byte header (last-block flag + type) + 3-byte big-endian
    /// length + the data.
    fn flac_block(block_type: u8, data: &[u8], is_last: bool) -> Vec<u8> {
        let mut b = Vec::with_capacity(4 + data.len());
        let mut header = block_type & 0x7F;
        if is_last {
            header |= 0x80;
        }
        b.push(header);
        b.extend_from_slice(&(data.len() as u32).to_be_bytes()[1..]);
        b.extend_from_slice(data);
        b
    }

    /// 34 dummy bytes standing in for a STREAMINFO block's fixed-size payload (its actual field layout is
    /// irrelevant to this codec — it's opaque data that must simply survive unchanged).
    fn streaminfo_data() -> Vec<u8> {
        vec![0xABu8; 34]
    }

    /// Concatenate the `fLaC` magic, a sequence of already-serialized metadata blocks, and trailing "audio"
    /// bytes into one stream.
    fn build_flac_stream(blocks: &[Vec<u8>], audio: &[u8]) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(b"fLaC");
        for b in blocks {
            f.extend_from_slice(b);
        }
        f.extend_from_slice(audio);
        f
    }

    #[test]
    fn write_vorbis_comment_round_trips_representative_fields() {
        let fields = vec![
            vfield("Title", "Awake"),
            vfield("Artist", "Tycho"),
            vfield("Album", "Dive"),
            vfield("Album Artist", "Tycho"),
            vfield("Track", "3"),
            vfield("Disc", "1"),
            vfield("Genre", "Ambient"),
            vfield("Year", "2011"),
            vfield("Composer", "Scott Hansen"),
            vfield("Publisher", "Ghostly International"),
            vfield("Comment", "Great album"),
            vfield("BPM", "120"),
            vfield("Copyright", "2011 Ghostly"),
        ];
        let block = write_vorbis_comment(&fields);
        let parsed = parse_vorbis_comment(&block);
        for f in &fields {
            assert_eq!(get(&parsed, &f.key), Some(f.value.as_str()), "key {} did not round-trip", f.key);
        }
    }

    #[test]
    fn write_vorbis_comment_skips_wrong_group_blank_values_and_unmappable_keys() {
        let fields = vec![
            vfield("Title", "Kept"),
            MetaField { group: "id3".to_string(), key: "Artist".to_string(), value: "Ignored".to_string(), editable: true },
            vfield("Album", ""), // blank value, skipped
            vfield("Some Random Thing", "value"), // unknown key with a space isn't a plausible raw key
        ];
        let block = write_vorbis_comment(&fields);
        let parsed = parse_vorbis_comment(&block);
        assert_eq!(parsed.len(), 1);
        assert_eq!(get(&parsed, "Title"), Some("Kept"));
    }

    #[test]
    fn write_vorbis_comment_passes_through_unknown_capitalised_keys() {
        // Inverse of `parse_vorbis_comment`'s `capitalise` fallback for an unrecognised comment name.
        let fields = vec![vfield("Replaygain_track_gain", "-6.5 dB")];
        let block = write_vorbis_comment(&fields);
        let parsed = parse_vorbis_comment(&block);
        assert_eq!(get(&parsed, "Replaygain_track_gain"), Some("-6.5 dB"));
    }

    #[test]
    fn write_flac_replaces_existing_vorbis_comment_block_without_duplicating() {
        let streaminfo = flac_block(0, &streaminfo_data(), false);
        let old_comment = write_vorbis_comment(&[vfield("Title", "Old Title")]);
        let comment_block = flac_block(4, &old_comment, true);
        let audio = b"AUDIOFRAMESDATA".to_vec();
        let orig = build_flac_stream(&[streaminfo.clone(), comment_block], &audio);

        // Sanity: the reader sees the old title before we touch anything.
        assert_eq!(get(&read_flac(&orig), "Title"), Some("Old Title"));

        let written = write_flac(&orig, &[vfield("Title", "New Title"), vfield("Artist", "New Artist")]);

        // STREAMINFO preserved byte-for-byte (its position/last-flag doesn't change: still block 0 of 2).
        assert_eq!(&written[4..4 + streaminfo.len()], &streaminfo[..]);
        // Audio frames preserved byte-for-byte at the tail.
        assert!(written.ends_with(&audio[..]));

        let read_back = read_flac(&written);
        assert_eq!(get(&read_back, "Title"), Some("New Title"));
        assert_eq!(get(&read_back, "Artist"), Some("New Artist"));

        // Exactly one STREAMINFO + one VORBIS_COMMENT block — no duplicate comment block — and the
        // last-block flag lands on the true final block (parse_flac_blocks itself enforces that shape).
        let (blocks, audio_start) = parse_flac_blocks(&written).expect("rewritten stream must parse cleanly");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].block_type, 0);
        assert_eq!(blocks[1].block_type, 4);
        assert_eq!(&written[audio_start..], &audio[..]);
    }

    #[test]
    fn write_flac_inserts_comment_block_after_streaminfo_when_absent() {
        let streaminfo = flac_block(0, &streaminfo_data(), true); // only block, and last, initially
        let audio = b"SOMEAUDIOBYTES".to_vec();
        let orig = build_flac_stream(&[streaminfo], &audio);

        assert!(read_flac(&orig).is_empty()); // sanity: no comment block yet

        let written = write_flac(&orig, &[vfield("Title", "Fresh Tag")]);

        // STREAMINFO's data preserved byte-for-byte (offset 4 magic + 4 header = 8).
        assert_eq!(&written[8..8 + 34], &streaminfo_data()[..]);

        let read_back = read_flac(&written);
        assert_eq!(get(&read_back, "Title"), Some("Fresh Tag"));

        let (blocks, audio_start) = parse_flac_blocks(&written).expect("rewritten stream must parse cleanly");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].block_type, 0, "STREAMINFO must stay first");
        assert_eq!(blocks[1].block_type, 4, "comment block inserted right after STREAMINFO");
        assert_eq!(&written[audio_start..], &audio[..]);
    }

    #[test]
    fn write_flac_preserves_other_metadata_blocks_byte_for_byte() {
        let streaminfo = flac_block(0, &streaminfo_data(), false);
        let padding_data = vec![0u8; 16];
        let padding = flac_block(1, &padding_data, false); // PADDING, an arbitrary "other" block type
        let old_comment = write_vorbis_comment(&[vfield("Title", "Old")]);
        let comment_block = flac_block(4, &old_comment, true);
        let audio = b"AUDIO".to_vec();
        let orig = build_flac_stream(&[streaminfo.clone(), padding.clone(), comment_block], &audio);

        let written = write_flac(&orig, &[vfield("Title", "New")]);

        // Both STREAMINFO and PADDING blocks are byte-identical, headers included (neither is last before
        // or after the rewrite, and the middle block's data is untouched).
        assert_eq!(&written[4..4 + streaminfo.len()], &streaminfo[..]);
        assert_eq!(&written[4 + streaminfo.len()..4 + streaminfo.len() + padding.len()], &padding[..]);

        let read_back = read_flac(&written);
        assert_eq!(get(&read_back, "Title"), Some("New"));
        assert!(written.ends_with(&audio[..]));

        let (blocks, _) = parse_flac_blocks(&written).expect("rewritten stream must parse cleanly");
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].block_type, 0);
        assert_eq!(blocks[1].block_type, 1);
        assert_eq!(blocks[2].block_type, 4);
    }

    #[test]
    fn write_flac_writing_twice_is_idempotent_payload_wise() {
        let streaminfo = flac_block(0, &streaminfo_data(), true);
        let audio = b"AUDIODATAHERE".to_vec();
        let orig = build_flac_stream(&[streaminfo], &audio);
        let fields = vec![vfield("Title", "A"), vfield("Artist", "B")];
        let once = write_flac(&orig, &fields);
        let twice = write_flac(&once, &fields);
        assert_eq!(once, twice, "re-writing over an already-written FLAC must produce identical bytes");
    }

    #[test]
    fn full_round_trip_read_edit_write_read() {
        use crate::media_meta_edit::{apply_edits, MetaEdit};

        let streaminfo = flac_block(0, &streaminfo_data(), false);
        let old_comment = write_vorbis_comment(&[vfield("Title", "Old Title"), vfield("Artist", "Old Artist")]);
        let comment_block = flac_block(4, &old_comment, true);
        let audio = b"AUDIO-FRAMES".to_vec();
        let orig = build_flac_stream(&[streaminfo, comment_block], &audio);

        let read_fields = read_flac(&orig);
        let edit_result = apply_edits(
            &read_fields,
            &[MetaEdit::Set { group: "vorbis".to_string(), key: "Title".to_string(), value: "Edited Title".to_string() }],
        );
        assert!(edit_result.rejected.is_empty());

        let written = write_flac(&orig, &edit_result.fields);
        let final_fields = read_flac(&written);
        assert_eq!(get(&final_fields, "Title"), Some("Edited Title"));
        assert_eq!(get(&final_fields, "Artist"), Some("Old Artist"));
    }

    #[test]
    fn write_flac_falls_back_to_orig_unchanged_on_malformed_or_missing_magic() {
        let malformed_with_magic: [&[u8]; 4] = [
            b"fLaC",                 // magic only, no block header at all
            b"fLaC\x00",             // 1 byte into a block header
            b"fLaC\x00\x00\x00",     // 3 bytes into a block header
            b"fLaC\x84\x00\x00\xFF", // last-block flag set, but claims 0xFF bytes of data that aren't there
        ];
        for orig in malformed_with_magic {
            let out = write_flac(orig, &[vfield("Title", "X")]);
            assert_eq!(out, orig.to_vec(), "malformed FLAC block chain must be returned unchanged");
        }

        let no_magic: [&[u8]; 4] =
            [b"", b"f", b"not a flac file at all, just noise", &[0xFFu8, 0xFB, 0x90, 0x00]];
        for orig in no_magic {
            let out = write_flac(orig, &[vfield("Title", "X")]);
            assert_eq!(out, orig.to_vec());
        }
    }

    #[test]
    fn write_flac_never_panics_on_any_truncation_of_a_valid_stream() {
        let streaminfo = flac_block(0, &streaminfo_data(), true);
        let valid = build_flac_stream(&[streaminfo], b"audio-bytes-here");
        for cut in 0..valid.len() {
            let _ = write_flac(&valid[..cut], &[vfield("Title", "X")]); // must never panic
        }
    }

    // ---- EXIF / JPEG write-back (CPE-1288) ----

    fn efield(key: &str, value: &str) -> MetaField {
        MetaField { group: "exif".to_string(), key: key.to_string(), value: value.to_string(), editable: true }
    }

    /// A JPEG of `SOI + COM(comment) + EOI` — the COM + EOI tail is our "must survive byte-for-byte"
    /// witness (it stands in for every non-EXIF segment + the scan data).
    fn jpeg_with_comment(comment: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut seed = vec![0xFFu8, 0xD8];
        seed.extend_from_slice(&[0xFF, 0xFE]); // COM marker
        seed.extend_from_slice(&((comment.len() + 2) as u16).to_be_bytes());
        seed.extend_from_slice(comment);
        seed.extend_from_slice(&[0xFF, 0xD9]); // EOI
        let tail = seed[2..].to_vec(); // everything after SOI = COM + EOI
        (seed, tail)
    }

    #[test]
    fn write_exif_errs_on_non_jpeg_without_panicking() {
        for orig in [&b""[..], &b"\xFF"[..], &b"\xFF\x00"[..], &b"\x89PNG\r\n\x1a\n"[..], &b"just text"[..]] {
            assert!(write_exif(orig, &[efield("Artist", "X")]).is_err());
        }
    }

    #[test]
    fn write_exif_errs_when_no_editable_field_is_present() {
        let seed = [0xFFu8, 0xD8, 0xFF, 0xD9];
        let fields = vec![
            // read-only intrinsic + wrong group + wrong key → nothing writable → Err (empty IFD).
            MetaField { group: "exif".into(), key: "Make".into(), value: "Acme".into(), editable: false },
            MetaField { group: "id3".into(), key: "Title".into(), value: "N".into(), editable: true },
        ];
        assert!(write_exif(&seed, &fields).is_err());
    }

    #[test]
    fn write_exif_round_trips_editable_tags_through_the_reader() {
        use crate::media_meta_read::read_exif;
        let seed = [0xFFu8, 0xD8, 0xFF, 0xD9];
        let jpg = write_exif(
            &seed,
            &[efield("ImageDescription", "Hello"), efield("Artist", "Me"), efield("Copyright", "2026")],
        )
        .unwrap();
        assert!(jpg.starts_with(&[0xFF, 0xD8, 0xFF, 0xE1]));
        let fields = read_exif(&jpg);
        let g = |k: &str| fields.iter().find(|f| f.key == k).map(|f| f.value.as_str());
        // The reader renders ASCII tags with surrounding quotes.
        assert_eq!(g("ImageDescription"), Some("\"Hello\""));
        assert_eq!(g("Artist"), Some("\"Me\""));
        assert_eq!(g("Copyright"), Some("\"2026\""));
    }

    #[test]
    fn write_exif_replaces_prior_exif_app1_and_preserves_other_segments() {
        let (seed, tail) = jpeg_with_comment(b"keep-this-comment");
        let v1 = write_exif(&seed, &[efield("Artist", "First")]).unwrap();
        // APP1 inserted right after SOI; COM + EOI preserved byte-for-byte.
        assert!(v1.starts_with(&[0xFF, 0xD8, 0xFF, 0xE1]));
        assert!(v1.ends_with(&tail[..]));

        // Re-writing replaces the APP1 rather than stacking a second one.
        let v2 = write_exif(&v1, &[efield("Artist", "Second")]).unwrap();
        assert!(v2.ends_with(&tail[..]));
        let exif_app1_count = v2.windows(6).filter(|w| *w == b"Exif\0\0").count();
        assert_eq!(exif_app1_count, 1, "must replace, not stack, the Exif APP1");

        use crate::media_meta_read::read_exif;
        assert_eq!(
            read_exif(&v2).iter().find(|f| f.key == "Artist").map(|f| f.value.as_str()),
            Some("\"Second\"")
        );
    }

    #[test]
    fn write_exif_keeps_a_leading_app0_jfif_ahead_of_the_new_app1() {
        // SOI + APP0(JFIF) + EOI. The APP1 must land after APP0, not before it.
        let mut seed = vec![0xFFu8, 0xD8, 0xFF, 0xE0];
        let jfif = b"JFIF\0\x01\x02\x00\x00\x01\x00\x01\x00\x00"; // minimal APP0 body
        seed.extend_from_slice(&((jfif.len() + 2) as u16).to_be_bytes());
        seed.extend_from_slice(jfif);
        seed.extend_from_slice(&[0xFF, 0xD9]);
        let out = write_exif(&seed, &[efield("Artist", "X")]).unwrap();
        // Order: SOI, APP0, APP1(Exif), EOI.
        assert_eq!(&out[0..4], &[0xFF, 0xD8, 0xFF, 0xE0]);
        let app0_end = 4 + 2 + jfif.len();
        assert_eq!(&out[app0_end..app0_end + 2], &[0xFF, 0xE1]);
    }

    #[test]
    fn write_exif_writing_twice_is_idempotent() {
        let (seed, _) = jpeg_with_comment(b"c");
        let fields = [efield("ImageDescription", "Cap"), efield("Artist", "A")];
        let once = write_exif(&seed, &fields).unwrap();
        let twice = write_exif(&once, &fields).unwrap();
        assert_eq!(once, twice, "re-writing over an already-written EXIF must produce identical bytes");
    }

    #[test]
    fn write_exif_never_panics_on_any_truncation_of_a_valid_jpeg() {
        let (seed, _) = jpeg_with_comment(b"comment");
        let valid = write_exif(&seed, &[efield("Artist", "X")]).unwrap();
        for cut in 0..valid.len() {
            let _ = write_exif(&valid[..cut], &[efield("Artist", "Y")]); // must never panic
        }
    }

    #[test]
    fn unquote_display_ascii_inverts_the_readers_formatting() {
        assert_eq!(unquote_display_ascii("\"Hello\""), "Hello");
        assert_eq!(unquote_display_ascii("plain"), "plain"); // not display-quoted → unchanged
        assert_eq!(unquote_display_ascii("\"a\\\"b\""), "a\"b"); // \" → "
        assert_eq!(unquote_display_ascii("\"a\\\\b\""), "a\\b"); // \\ → \
        assert_eq!(unquote_display_ascii("\"\\x41BC\""), "ABC"); // \x41 → A
    }
}
