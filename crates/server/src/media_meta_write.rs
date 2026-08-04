//! Media-metadata **write codecs** (CPE-1035 ID3v2, CPE-1038 FLAC/Vorbis; epic CPE-725): the counterpart
//! to [`crate::media_meta_read`]'s read side. [`crate::media_meta_edit`] owns the edit *policy* over
//! [`MetaField`]s but does no file parsing/writing — "the codec layer reads the fields in and writes the
//! result back." [`crate::media_meta_read`] is that read layer; this module is the write layer:
//! - [`write_id3v2`] builds a fresh, valid **ID3v2.4** tag from the `group == "id3"` fields and prepends it
//!   to the original audio payload, replacing any pre-existing tag rather than stacking on top of it.
//! - [`write_flac`] rebuilds a **FLAC** stream with its `VORBIS_COMMENT` metadata block replaced by one
//!   built from the `group == "vorbis"` fields (via [`write_vorbis_comment`]), preserving STREAMINFO,
//!   every other metadata block, and the audio frames byte-for-byte.
//! - [`write_ogg`] rebuilds an **Ogg Vorbis** stream (CPE-1289) with its comment-header packet replaced,
//!   **re-paging** the header region (re-laced segment tables, reassigned page sequence numbers, and
//!   recomputed per-page CRC32 — Ogg's non-reflected `0x04c11db7` polynomial, by hand) while preserving the
//!   identification header and every audio page.
//! - [`write_wav`] rebuilds a **WAV/RIFF** stream (CPE-1298) with its `LIST`/`INFO` chunk replaced (or
//!   inserted, if absent) with one built from the `group == "wav"` fields, preserving every other chunk
//!   (`fmt `, `data`, …) byte-for-byte and updating the outer `RIFF` size to match.
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
/// **Every other existing EXIF field is preserved.** The block is rebuilt by re-parsing `orig`'s current
/// EXIF and re-emitting all of the primary image's fields (IFD0 + the Exif / GPS / Interop sub-IFDs),
/// then *overriding* only the four editable tags with the new values — so intrinsics the reader marks
/// read-only (Orientation, GPS geotag, DateTimeOriginal, Make/Model, exposure, …) survive a caption edit
/// instead of being discarded. Read values are un-quoted first (the reader renders ASCII tags with
/// surrounding quotes via `display_value`), so an editable field carried through unchanged round-trips;
/// UserComment is written with the standard 8-byte `ASCII\0\0\0` character-code prefix, or, if the value
/// is still the reader's `0x…` hex form, decoded back to its raw bytes so it round-trips too. (The
/// thumbnail image — IFD1 / `In::THUMBNAIL` — is the one thing not carried, since its JPEG bytes can't be
/// threaded through this writer; thumbnails are regenerable, not intrinsic metadata.)
///
/// Returns `Err` (never panics) for non-JPEG/truncated input, when there are no editable EXIF fields to
/// write, or when the encoded Exif block won't fit a single 64 KiB APP1 segment. **JPEG only** — for a
/// TIFF the EXIF *is* the file's own IFD chain (rewriting it would drop the image strips/thumbnail), so
/// `tif`/`tiff` are deliberately left out of [`crate::media_meta::is_writable`] for now.
pub fn write_exif(orig: &[u8], fields: &[MetaField]) -> Result<Vec<u8>, String> {
    if orig.len() < 2 || orig[0] != 0xFF || orig[1] != 0xD8 {
        return Err("not a JPEG (missing SOI marker)".into());
    }
    let tiff = build_exif_tiff(orig, fields)?;

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

/// Build a raw TIFF/Exif block (big-endian) via `kamadak-exif`'s experimental `Writer`, preserving every
/// existing field of `orig`'s primary image (IFD0 + the Exif / GPS / Interop sub-IFDs) and overriding only
/// the four editable tags with the new values from `fields`. Re-parsing `orig` (rather than trusting the
/// `MetaField` list, which only carries the tags [`crate::media_meta_read::read_exif`] surfaces) is what
/// keeps intrinsics — Orientation, GPS, DateTimeOriginal, Make/Model, exposure — from being lost on an
/// edit. The thumbnail image (`In::THUMBNAIL`) is dropped because its JPEG bytes can't be carried through
/// this writer. `Err` if no editable field is present (an Exif block must have at least one field, and we
/// never write an intrinsics-only block that just re-encodes what was already there) or the encoder fails.
fn build_exif_tiff(orig: &[u8], fields: &[MetaField]) -> Result<Vec<u8>, String> {
    let overrides = build_override_fields(fields);
    if overrides.is_empty() {
        return Err("no editable EXIF fields to write".into());
    }
    // Existing EXIF (if any) whose non-overridden fields we carry through. `.ok()` → None for a JPEG with
    // no EXIF yet (e.g. a freshly-created image), in which case only the overrides are written.
    let existing = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(orig)).ok();

    let mut writer = exif::experimental::Writer::new();
    if let Some(ref exif) = existing {
        for f in exif.fields() {
            // Preserve only the primary image's IFDs; skip the thumbnail image (its data can't be carried).
            if f.ifd_num != exif::In::PRIMARY {
                continue;
            }
            // Don't re-emit a stale copy of a tag we're about to override (would duplicate the entry).
            if overrides.iter().any(|o| o.tag == f.tag) {
                continue;
            }
            writer.push_field(f);
        }
    }
    for f in &overrides {
        writer.push_field(f);
    }

    let mut cursor = std::io::Cursor::new(Vec::new());
    writer.write(&mut cursor, false).map_err(|e| format!("failed to encode EXIF: {e}"))?;
    Ok(cursor.into_inner())
}

/// Translate the `"exif"`-group, editable fields into the `exif::Field` overrides the `Writer` applies on
/// top of the preserved existing IFDs. Only the four editable tags are emitted; blank values, other
/// groups, and camera-intrinsic keys are skipped (mirrors [`build_frames`]'s handling of the `"id3"`
/// group). ASCII tags are un-quoted; UserComment gets its standard character-code prefix (see
/// [`user_comment_bytes`]).
fn build_override_fields(fields: &[MetaField]) -> Vec<exif::Field> {
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

// ---- OGG / Vorbis comment write-back (CPE-1289) ----

/// The three Vorbis header-packet type+signature prefixes (`\x01vorbis` / `\x03vorbis` / `\x05vorbis`) —
/// identification, comment, and setup respectively. Mirrors [`crate::media_meta_read`]'s
/// `OGG_VORBIS_COMMENT_SIG`.
const OGG_IDENT_SIG: &[u8] = b"\x01vorbis";
const OGG_COMMENT_SIG: &[u8] = b"\x03vorbis";
const OGG_SETUP_SIG: &[u8] = b"\x05vorbis";

/// CRC-32 lookup table for Ogg's page checksum: polynomial `0x04c11db7`, MSB-first, **no** input/output
/// reflection, initial value 0, no final xor. Built at compile time so there's no runtime table setup and
/// no dependency. (This is deliberately *not* the reflected CRC-32 used by zlib/PNG.)
const OGG_CRC_TABLE: [u32; 256] = build_ogg_crc_table();

const fn build_ogg_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut r = (i as u32) << 24;
        let mut j = 0;
        while j < 8 {
            r = if r & 0x8000_0000 != 0 { (r << 1) ^ 0x04c1_1db7 } else { r << 1 };
            j += 1;
        }
        table[i] = r;
        i += 1;
    }
    table
}

/// Compute Ogg's page CRC over `data`. The caller must have zeroed the 4-byte CRC field (offset 22..26)
/// within `data` first — Ogg checksums the *entire* page with that field cleared.
fn ogg_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &b in data {
        let idx = (((crc >> 24) as u8) ^ b) as usize;
        crc = (crc << 8) ^ OGG_CRC_TABLE[idx];
    }
    crc
}

/// One parsed Ogg page: its byte range within the original buffer plus the header fields this codec needs.
/// `body_start`/`body_len` delimit the concatenated segment bodies (right after the segment table).
struct OggPage {
    start: usize,
    end: usize,
    header_type: u8,
    serial: u32,
    seg_table: Vec<u8>,
    body_start: usize,
    body_len: usize,
}

/// Parse `bytes` into its sequence of Ogg pages, bounds-checked throughout — a truncated header, segment
/// table, or body yields `Err` rather than a panic. Each page's declared body length is the sum of its
/// lacing values (the segment table). `Err` for anything that isn't a clean run of `OggS` pages.
fn parse_ogg_pages(bytes: &[u8]) -> Result<Vec<OggPage>, String> {
    let mut pages = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let header = bytes.get(pos..pos + 27).ok_or("truncated Ogg page header")?;
        if &header[0..4] != b"OggS" {
            return Err("not an Ogg stream (bad capture pattern)".into());
        }
        let header_type = header[5];
        let serial = u32::from_le_bytes([header[14], header[15], header[16], header[17]]);
        let page_segments = header[26] as usize;
        let seg_table_start = pos + 27;
        let seg_table =
            bytes.get(seg_table_start..seg_table_start + page_segments).ok_or("truncated Ogg segment table")?.to_vec();
        let body_start = seg_table_start + page_segments;
        let body_len: usize = seg_table.iter().map(|&l| l as usize).sum();
        let end = body_start + body_len;
        if end > bytes.len() {
            return Err("truncated Ogg page body".into());
        }
        pages.push(OggPage { start: pos, end, header_type, serial, seg_table, body_start, body_len });
        pos = end;
    }
    if pages.is_empty() {
        return Err("no Ogg pages found".into());
    }
    Ok(pages)
}

/// Lace `packets` into Ogg segments (each packet → floor(len/255) full 255-byte segments + one terminator
/// segment of `len % 255` bytes, so an exact-multiple length still gets a trailing 0-lace terminator),
/// split them into pages of at most 255 segments, and append the serialized pages to `out`. The first page
/// gets `first_flags` (e.g. `0x02` BOS); any later page produced by this call is a **continuation** page
/// (`0x01`) iff the previous page's final lacing value was `255` (the packet ran past the page boundary),
/// else `0x00`. Header pages carry granule position 0 (per the Vorbis spec). `seq` is advanced past every
/// page emitted. Each page's CRC is computed last, over the finished page with its CRC field zeroed.
fn page_out(out: &mut Vec<u8>, packets: &[&[u8]], serial: u32, seq: &mut u32, first_flags: u8) {
    let mut segs: Vec<(u8, &[u8])> = Vec::new();
    for &pkt in packets {
        let mut rest = pkt;
        loop {
            if rest.len() >= 255 {
                segs.push((255, &rest[..255]));
                rest = &rest[255..];
            } else {
                segs.push((rest.len() as u8, rest));
                break;
            }
        }
    }

    let mut i = 0usize;
    let mut page_index = 0usize;
    let mut prev_last_lace = 0u8;
    while i < segs.len() {
        let take = (segs.len() - i).min(255);
        let chunk = &segs[i..i + take];
        let flags = if page_index == 0 {
            first_flags
        } else if prev_last_lace == 255 {
            0x01 // continued packet spilled onto this page
        } else {
            0x00
        };
        emit_page(out, flags, 0, serial, *seq, chunk);
        *seq += 1;
        prev_last_lace = chunk.last().map(|&(l, _)| l).unwrap_or(0);
        i += take;
        page_index += 1;
    }
}

/// Serialize one Ogg page (27-byte header + segment table + segment bodies) to the tail of `out` and fill
/// in its CRC. `granule` is the page's granule position (0 for header pages).
fn emit_page(out: &mut Vec<u8>, flags: u8, granule: u64, serial: u32, seq: u32, segs: &[(u8, &[u8])]) {
    let start = out.len();
    out.extend_from_slice(b"OggS");
    out.push(0); // stream structure version
    out.push(flags);
    out.extend_from_slice(&granule.to_le_bytes());
    out.extend_from_slice(&serial.to_le_bytes());
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]); // CRC placeholder, filled in below
    out.push(segs.len() as u8);
    for &(lace, _) in segs {
        out.push(lace);
    }
    for &(_, body) in segs {
        out.extend_from_slice(body);
    }
    let crc = ogg_crc32(&out[start..]);
    out[start + 22..start + 26].copy_from_slice(&crc.to_le_bytes());
}

/// Rebuild an Ogg Vorbis stream (`orig`) with its Vorbis **comment header** (the 2nd of the three Vorbis
/// header packets) replaced by a fresh one built from the `"vorbis"`-group fields in `fields` (via
/// [`write_vorbis_comment`]), then **re-page** the header region: the identification header is kept alone on
/// the beginning-of-stream page, the new comment + the original setup header are re-laced onto the following
/// page(s), page sequence numbers are reassigned, and every rebuilt page's CRC32 is recomputed. The
/// identification header packet and all audio pages are preserved (audio pages byte-for-byte apart from
/// their renumbered sequence field + recomputed CRC — granule positions, serial, and header-type/EOS flags
/// are untouched).
///
/// Scope: a single logical Vorbis bitstream (standard `.ogg`/`.oga` audio). Multiplexed/chained streams
/// (a page whose serial differs from the first page's) are refused with `Err` rather than risk corrupting
/// another stream's page numbering. `Err` (never a panic) for non-Ogg/truncated/malformed input, a stream
/// with no complete Vorbis header packets, non-Vorbis header signatures, or a non-standard layout where
/// audio data shares the setup header's page.
pub fn write_ogg(orig: &[u8], fields: &[MetaField]) -> Result<Vec<u8>, String> {
    if orig.len() < 4 || &orig[0..4] != b"OggS" {
        return Err("not an Ogg stream (missing OggS capture pattern)".into());
    }
    let pages = parse_ogg_pages(orig)?;

    // Single logical bitstream only — refuse multiplexed/chained streams (each logical stream owns its own
    // per-serial page numbering, which re-paging the Vorbis headers would disturb).
    let serial = pages[0].serial;
    if pages.iter().any(|p| p.serial != serial) {
        return Err("multiplexed/chained Ogg streams are not supported for metadata write".into());
    }
    if pages[0].header_type & 0x02 == 0 {
        return Err("first Ogg page is not a beginning-of-stream page".into());
    }

    // Reassemble the first three Vorbis header packets (identification, comment, setup) by concatenating
    // segment bodies across pages, and record the page where the setup header terminates — the boundary
    // between the header pages we rebuild and the audio pages we preserve.
    let mut cur: Vec<u8> = Vec::new();
    let mut packets: Vec<Vec<u8>> = Vec::new();
    let mut last_hdr_page: Option<usize> = None;
    'outer: for (pi, page) in pages.iter().enumerate() {
        let body = &orig[page.body_start..page.body_start + page.body_len];
        let mut off = 0usize;
        for &lace in &page.seg_table {
            cur.extend_from_slice(&body[off..off + lace as usize]);
            off += lace as usize;
            if lace < 255 {
                packets.push(std::mem::take(&mut cur));
                if packets.len() == 3 {
                    // The setup header must be the last packet on its page: Vorbis audio starts on a fresh
                    // page, so leftover segment data here means a layout we won't risk re-paging.
                    if off != page.body_len {
                        return Err("audio data shares the Vorbis setup-header page (unsupported layout)".into());
                    }
                    last_hdr_page = Some(pi);
                    break 'outer;
                }
            }
        }
    }
    let last_hdr_page = last_hdr_page.ok_or("Ogg stream has no complete Vorbis header packets")?;

    let ident = &packets[0];
    let setup = &packets[2];
    if !ident.starts_with(OGG_IDENT_SIG) || !packets[1].starts_with(OGG_COMMENT_SIG) || !setup.starts_with(OGG_SETUP_SIG)
    {
        return Err("not a Vorbis stream (unexpected header-packet signatures)".into());
    }

    // Fresh comment packet: 0x03 "vorbis" + freshly built comment data + the Vorbis framing bit (0x01),
    // exactly the shape a decoder expects (the reader ignores the trailing framing byte).
    let mut new_comment = OGG_COMMENT_SIG.to_vec();
    new_comment.extend_from_slice(&write_vorbis_comment(fields));
    new_comment.push(0x01);

    let mut out = Vec::with_capacity(orig.len() + new_comment.len());
    let mut seq = 0u32;
    // Identification header alone on the BOS page (as the Vorbis spec requires), then comment + setup.
    page_out(&mut out, &[ident], serial, &mut seq, 0x02);
    page_out(&mut out, &[&new_comment, setup], serial, &mut seq, 0x00);

    // Preserve every audio page byte-for-byte apart from a renumbered sequence field + recomputed CRC.
    for page in &pages[last_hdr_page + 1..] {
        let pstart = out.len();
        out.extend_from_slice(&orig[page.start..page.end]);
        out[pstart + 18..pstart + 22].copy_from_slice(&seq.to_le_bytes());
        out[pstart + 22..pstart + 26].copy_from_slice(&[0, 0, 0, 0]);
        let crc = ogg_crc32(&out[pstart..]);
        out[pstart + 22..pstart + 26].copy_from_slice(&crc.to_le_bytes());
        seq += 1;
    }
    Ok(out)
}

// ---- WAV / RIFF-INFO write-back (CPE-1298) ----

/// One parsed top-level RIFF chunk: its 4-byte id and the byte range of its *data* (header excluded)
/// within the original buffer.
struct RiffChunk {
    id: [u8; 4],
    data_start: usize,
    data_end: usize,
}

/// Parse `bytes`' top-level RIFF chunk tree, starting right after the `RIFF`+size+`WAVE` header (offset
/// 12 — checked by the caller). Each chunk is a 4-byte id + little-endian 4-byte size + that many data
/// bytes, padded to an even length (the pad byte, if any, is not part of the next chunk — mirrors
/// [`crate::media_meta_read::read_wav`]'s walk). `Err` for a chunk whose declared size runs past the end
/// of the buffer (truncated/corrupt input); trailing bytes too short to hold another chunk header are
/// silently ignored, same as the reader. Never panics.
fn parse_riff_chunks(bytes: &[u8]) -> Result<Vec<RiffChunk>, String> {
    let mut chunks = Vec::new();
    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let id_bytes = bytes.get(pos..pos + 4).ok_or("truncated RIFF chunk header")?;
        let size_bytes = bytes.get(pos + 4..pos + 8).ok_or("truncated RIFF chunk header")?;
        let size = le_u32(size_bytes) as usize;
        let data_start = pos + 8;
        let data_end = data_start.checked_add(size).ok_or("RIFF chunk size overflow")?;
        if data_end > bytes.len() {
            return Err("RIFF chunk runs past end of file (truncated/corrupt WAV)".into());
        }
        let mut id = [0u8; 4];
        id.copy_from_slice(id_bytes);
        chunks.push(RiffChunk { id, data_start, data_end });
        let padded_size = if size % 2 == 0 { size } else { size + 1 };
        let next = data_start.checked_add(padded_size).ok_or("RIFF chunk padding overflow")?;
        pos = next;
    }
    Ok(chunks)
}

/// Read a little-endian u32 from 4 bytes (mirrors `media_meta_read`'s private helper; re-implemented
/// locally for the same reason as [`syncsafe28_decode`]/[`be_u24`]).
fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// Map a friendly key (as produced by [`crate::media_meta_read::wav_info_key`]) back to its raw 4-char
/// RIFF `INFO` sub-chunk id. The inverse of that private mapping (re-implemented locally, same reasoning
/// as this module's other codecs). A friendly key outside the known set is skipped — RIFF INFO defines
/// many more ids than [`crate::media_meta_read::read_wav`] chooses to surface, so there's no raw-id
/// passthrough to invert.
fn wav_field_id(key: &str) -> Option<&'static [u8; 4]> {
    match key {
        "Title" => Some(b"INAM"),
        "Artist" => Some(b"IART"),
        "Album" => Some(b"IPRD"),
        "Year" => Some(b"ICRD"),
        "Comment" => Some(b"ICMT"),
        "Genre" => Some(b"IGNR"),
        _ => None,
    }
}

/// Build the sub-chunks of a `LIST`/`INFO` chunk (right after where its 4-byte `INFO` form type goes) from
/// the `group == "wav"` fields in `fields` — the inverse of [`crate::media_meta_read::parse_wav_info`].
/// Each recognised, non-blank field becomes one sub-chunk: a 4-byte id, a little-endian 4-byte size, the
/// value's UTF-8 bytes, and a NUL terminator (the conventional RIFF INFO string form), padded to an even
/// length. Unrecognised friendly keys and blank values are skipped (mirrors [`build_frames`]'s handling of
/// the `"id3"` group).
fn build_wav_info(fields: &[MetaField]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in fields {
        if f.group != "wav" || f.value.is_empty() {
            continue;
        }
        let Some(id) = wav_field_id(&f.key) else { continue };
        let mut value = f.value.as_bytes().to_vec();
        value.push(0); // NUL-terminated, the conventional RIFF INFO string form
        out.extend_from_slice(id);
        out.extend_from_slice(&(value.len() as u32).to_le_bytes());
        out.extend_from_slice(&value);
        if value.len() % 2 != 0 {
            out.push(0); // pad this sub-chunk to an even length
        }
    }
    out
}

/// Rebuild a WAV/RIFF stream (`orig`) with its `LIST`/`INFO` chunk replaced by a fresh one built from the
/// `"wav"`-group fields in `fields` (via [`build_wav_info`]) — inserted at the end of the chunk list if no
/// `LIST`/`INFO` chunk existed yet. Every other chunk (`fmt `, `data`, and any other `LIST` chunk whose
/// form type isn't `INFO`) is preserved byte-for-byte, and the outer `RIFF` size is recomputed to match
/// the new total. `Err` (never a panic) for input that isn't a well-formed `RIFF`…`WAVE` stream, or whose
/// chunk tree is truncated/corrupt.
pub fn write_wav(orig: &[u8], fields: &[MetaField]) -> Result<Vec<u8>, String> {
    if orig.len() < 12 || &orig[0..4] != b"RIFF" || &orig[8..12] != b"WAVE" {
        return Err("not a WAV file (missing RIFF/WAVE header)".into());
    }
    let chunks = parse_riff_chunks(orig)?;
    let info_data = build_wav_info(fields);
    let mut new_info = Vec::with_capacity(4 + info_data.len());
    new_info.extend_from_slice(b"INFO");
    new_info.extend_from_slice(&info_data);

    let mut out_chunks: Vec<([u8; 4], &[u8])> = Vec::with_capacity(chunks.len() + 1);
    let mut found = false;
    for c in &chunks {
        if !found && c.id == *b"LIST" && orig.get(c.data_start..c.data_start + 4) == Some(&b"INFO"[..]) {
            out_chunks.push((c.id, &new_info[..]));
            found = true;
        } else {
            out_chunks.push((c.id, &orig[c.data_start..c.data_end]));
        }
    }
    if !found {
        out_chunks.push((*b"LIST", &new_info[..]));
    }

    let mut body = Vec::with_capacity(4 + out_chunks.iter().map(|(_, d)| 8 + d.len() + (d.len() % 2)).sum::<usize>());
    body.extend_from_slice(b"WAVE");
    for (id, data) in &out_chunks {
        body.extend_from_slice(id);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        if data.len() % 2 != 0 {
            body.push(0); // pad this chunk to an even length
        }
    }

    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_meta_read::{parse_vorbis_comment, read_flac, read_id3v2, read_wav};

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

    /// Build a JPEG standing in for a real camera photo: IFD0 (ImageDescription + Make + Orientation) +
    /// GPS sub-IFD (a geotag), wrapped in an Exif APP1. Uses the exif `Writer` directly so the fixture is
    /// independent of the code under test.
    fn camera_photo_jpeg() -> Vec<u8> {
        let fields = vec![
            exif::Field {
                tag: exif::Tag::ImageDescription,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Ascii(vec![b"Old Caption".to_vec()]),
            },
            exif::Field {
                tag: exif::Tag::Make,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Ascii(vec![b"Acme".to_vec()]),
            },
            exif::Field {
                tag: exif::Tag::Orientation,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Short(vec![6]), // rotate 90° CW — portrait would display sideways if lost
            },
            exif::Field {
                tag: exif::Tag::GPSLatitudeRef,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Ascii(vec![b"N".to_vec()]),
            },
            exif::Field {
                tag: exif::Tag::GPSLatitude,
                ifd_num: exif::In::PRIMARY,
                value: exif::Value::Rational(vec![
                    exif::Rational { num: 37, denom: 1 },
                    exif::Rational { num: 46, denom: 1 },
                    exif::Rational { num: 30, denom: 1 },
                ]),
            },
        ];
        let mut w = exif::experimental::Writer::new();
        for f in &fields {
            w.push_field(f);
        }
        let mut cur = std::io::Cursor::new(Vec::new());
        w.write(&mut cur, false).unwrap();
        let tiff = cur.into_inner();

        let mut jpg = vec![0xFFu8, 0xD8];
        jpg.extend_from_slice(&[0xFF, 0xE1]);
        jpg.extend_from_slice(&((tiff.len() + 6 + 2) as u16).to_be_bytes());
        jpg.extend_from_slice(b"Exif\0\0");
        jpg.extend_from_slice(&tiff);
        jpg.extend_from_slice(&[0xFF, 0xD9]);
        jpg
    }

    /// Regression guard for the write-back data-loss bug: editing the caption of a real photo must NOT
    /// wipe the read-only camera intrinsics (Make / Orientation / GPS geotag). Before the fix,
    /// `write_exif` rebuilt the IFD from only the four editable tags and silently dropped everything else.
    #[test]
    fn write_exif_preserves_intrinsics_gps_and_orientation_when_editing_caption() {
        use crate::media_meta_read::read_exif;

        let photo = camera_photo_jpeg();
        // Sanity: the fixture really carries the intrinsics.
        let before = read_exif(&photo);
        let g = |fs: &[MetaField], k: &str| fs.iter().find(|f| f.key == k).map(|f| f.value.clone());
        assert_eq!(g(&before, "Make"), Some("\"Acme\"".to_string()));
        assert_eq!(g(&before, "ImageDescription"), Some("\"Old Caption\"".to_string()));
        assert!(g(&before, "Orientation").is_some());
        assert!(g(&before, "GPS Latitude").is_some());

        // Edit ONLY the caption (as write_back would: carry every read field through apply_edits).
        let edited: Vec<MetaField> = before
            .iter()
            .map(|f| {
                if f.key == "ImageDescription" {
                    MetaField { value: "New Caption".to_string(), ..f.clone() }
                } else {
                    f.clone()
                }
            })
            .collect();
        let out = write_exif(&photo, &edited).unwrap();
        let after = read_exif(&out);

        // The caption changed …
        assert_eq!(g(&after, "ImageDescription"), Some("\"New Caption\"".to_string()));
        // … and every intrinsic SURVIVED (this is the bug this test guards against).
        assert_eq!(g(&after, "Make"), Some("\"Acme\"".to_string()));
        assert_eq!(g(&after, "GPS Latitude"), g(&before, "GPS Latitude"));
        assert_eq!(g(&after, "Orientation"), g(&before, "Orientation"));
    }

    /// UserComment (an UNDEFINED-typed tag written with the `ASCII\0\0\0` character-code prefix) survives
    /// a write and reads back as the reader's hex rendering of those exact bytes.
    #[test]
    fn write_exif_round_trips_user_comment() {
        use crate::media_meta_read::read_exif;

        let seed = [0xFFu8, 0xD8, 0xFF, 0xD9];
        let jpg = write_exif(&seed, &[efield("UserComment", "Hi there")]).unwrap();
        let fields = read_exif(&jpg);
        let uc = fields.iter().find(|f| f.key == "UserComment").map(|f| f.value.clone());
        // Reader renders UNDEFINED as hex: "ASCII\0\0\0Hi there" = 41 53 43 49 49 00 00 00 48 69 …
        let mut expected = Vec::from(*b"ASCII\0\0\0");
        expected.extend_from_slice(b"Hi there");
        let expected_hex = format!("0x{}", expected.iter().map(|b| format!("{b:02x}")).collect::<String>());
        assert_eq!(uc, Some(expected_hex.clone()));

        // And it survives a second write unchanged (the `0x…` form is decoded back to raw bytes).
        let carried = vec![MetaField { group: "exif".into(), key: "UserComment".into(), value: expected_hex.clone(), editable: true }];
        let jpg2 = write_exif(&jpg, &carried).unwrap();
        let uc2 = read_exif(&jpg2).iter().find(|f| f.key == "UserComment").map(|f| f.value.clone());
        assert_eq!(uc2, Some(expected_hex));
    }

    // ---- OGG / Vorbis-comment write-back (CPE-1289) ----

    /// A from-scratch, bitwise (table-free) Ogg CRC — the reference the codec's table-driven [`ogg_crc32`]
    /// is cross-checked against. Two independent implementations agreeing is our CRC-correctness proof.
    fn ref_ogg_crc(data: &[u8]) -> u32 {
        let mut crc: u32 = 0;
        for &b in data {
            crc ^= (b as u32) << 24;
            for _ in 0..8 {
                crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04c1_1db7 } else { crc << 1 };
            }
        }
        crc
    }

    /// Lace one packet into `(lace, body)` segments (255-byte runs + a `< 255` terminator).
    fn lace_packet(packet: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let mut segs = Vec::new();
        let mut rest = packet;
        while rest.len() >= 255 {
            let (c, t) = rest.split_at(255);
            segs.push((255u8, c.to_vec()));
            rest = t;
        }
        segs.push((rest.len() as u8, rest.to_vec()));
        segs
    }

    /// Serialize one Ogg page independently of the code under test (its own CRC via [`ref_ogg_crc`]).
    fn build_page(flags: u8, granule: u64, serial: u32, seq: u32, segs: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(b"OggS");
        p.push(0);
        p.push(flags);
        p.extend_from_slice(&granule.to_le_bytes());
        p.extend_from_slice(&serial.to_le_bytes());
        p.extend_from_slice(&seq.to_le_bytes());
        p.extend_from_slice(&[0, 0, 0, 0]); // CRC placeholder
        p.push(segs.len() as u8);
        for (lace, _) in segs {
            p.push(*lace);
        }
        for (_, body) in segs {
            p.extend_from_slice(body);
        }
        let crc = ref_ogg_crc(&p);
        p[22..26].copy_from_slice(&crc.to_le_bytes());
        p
    }

    /// Build a minimal but valid single-stream Ogg Vorbis file: page 0 = identification header (BOS),
    /// page 1 = comment + setup headers, page 2 = one dummy audio page (EOS). Returns `(bytes, ident,
    /// audio_body)` so tests can assert those survive.
    fn build_ogg_vorbis(comment_fields: &[MetaField], serial: u32) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        // Identification header: 0x01 "vorbis" + 22 opaque bytes + framing bit = 30 bytes total.
        let mut ident = OGG_IDENT_SIG.to_vec();
        ident.extend_from_slice(&[0x11u8; 22]);
        ident.push(0x01);

        // Comment header: 0x03 "vorbis" + comment data + framing bit.
        let mut comment = OGG_COMMENT_SIG.to_vec();
        comment.extend_from_slice(&write_vorbis_comment(comment_fields));
        comment.push(0x01);

        // Setup header: 0x05 "vorbis" + opaque codebook stand-in.
        let mut setup = OGG_SETUP_SIG.to_vec();
        setup.extend_from_slice(&[0x5Au8; 40]);

        let audio_body = b"DUMMY-VORBIS-AUDIO-PACKET-BYTES".to_vec();

        let page0 = build_page(0x02, 0, serial, 0, &lace_packet(&ident));
        let mut cs_segs = lace_packet(&comment);
        cs_segs.extend(lace_packet(&setup));
        let page1 = build_page(0x00, 0, serial, 1, &cs_segs);
        // Audio page: EOS flag, a non-zero granule position that must be preserved verbatim.
        let page2 = build_page(0x04, 0xDEAD_BEEF, serial, 2, &lace_packet(&audio_body));

        let mut out = page0.clone();
        out.extend_from_slice(&page1);
        out.extend_from_slice(&page2);
        (out, ident, audio_body)
    }

    /// Walk `bytes` as Ogg pages and confirm every page's stored CRC matches an independent recomputation.
    /// Returns `(all_valid, page_count)`; a structurally broken stream yields `(false, _)`.
    fn all_page_crcs_valid(bytes: &[u8]) -> (bool, usize) {
        let mut pos = 0usize;
        let mut count = 0usize;
        while pos < bytes.len() {
            if pos + 27 > bytes.len() || &bytes[pos..pos + 4] != b"OggS" {
                return (false, count);
            }
            let nsegs = bytes[pos + 26] as usize;
            let seg_start = pos + 27;
            if seg_start + nsegs > bytes.len() {
                return (false, count);
            }
            let body_len: usize = bytes[seg_start..seg_start + nsegs].iter().map(|&l| l as usize).sum();
            let end = seg_start + nsegs + body_len;
            if end > bytes.len() {
                return (false, count);
            }
            let stored = u32::from_le_bytes([bytes[pos + 22], bytes[pos + 23], bytes[pos + 24], bytes[pos + 25]]);
            let mut page = bytes[pos..end].to_vec();
            page[22..26].copy_from_slice(&[0, 0, 0, 0]);
            if ref_ogg_crc(&page) != stored {
                return (false, count);
            }
            count += 1;
            pos = end;
        }
        (true, count)
    }

    #[test]
    fn ogg_crc32_matches_an_independent_bitwise_reference() {
        // The table-driven codec CRC must agree with the from-scratch bitwise CRC on assorted inputs …
        for data in [
            &b""[..],
            &b"OggS"[..],
            &b"\x00"[..],
            &b"123456789"[..],
            &[0xFFu8; 300][..],
            &b"the quick brown fox jumps over the lazy dog"[..],
        ] {
            assert_eq!(ogg_crc32(data), ref_ogg_crc(data), "CRC mismatch for {data:?}");
        }
        // … and a fixed known-answer vector pins the polynomial/endianness so a silent regression is caught.
        assert_eq!(ogg_crc32(b"123456789"), 0x89A1897F);
    }

    #[test]
    fn write_ogg_round_trips_a_comment_edit_and_preserves_ident_and_audio() {
        use crate::media_meta::{read_all, write_back};
        use crate::media_meta_edit::MetaEdit;

        let (orig, ident, audio_body) =
            build_ogg_vorbis(&[vfield("Title", "Old Title"), vfield("Artist", "Keep Artist")], 0x1234_5678);

        // Sanity: the reader sees the original tags and the fixture is a valid Ogg stream.
        assert_eq!(get(&read_all("ogg", &orig), "Title"), Some("Old Title"));
        assert_eq!(all_page_crcs_valid(&orig), (true, 3));

        let out = write_back(
            "ogg",
            &orig,
            &[MetaEdit::Set { group: "vorbis".into(), key: "Title".into(), value: "New Title".into() }],
        )
        .expect("ogg is writable");

        // The edit landed and the untouched field survived.
        let after = read_all("ogg", &out);
        assert_eq!(get(&after, "Title"), Some("New Title"));
        assert_eq!(get(&after, "Artist"), Some("Keep Artist"));

        // Identification header packet + audio packet body preserved.
        assert!(contains_subslice(&out, &ident), "identification header must survive");
        assert!(contains_subslice(&out, &audio_body), "audio data must survive");

        // Every page in the output validates, and the audio page kept its granule + EOS flag.
        let (valid, pages) = all_page_crcs_valid(&out);
        assert!(valid, "every output page CRC must validate");
        assert!(pages >= 3);
        // Locate the EOS/granule audio page (header_type 0x04) and confirm its granule is intact.
        let mut found_audio = false;
        let mut pos = 0usize;
        while pos < out.len() {
            let nsegs = out[pos + 26] as usize;
            let body_len: usize = out[pos + 27..pos + 27 + nsegs].iter().map(|&l| l as usize).sum();
            let end = pos + 27 + nsegs + body_len;
            if out[pos + 5] & 0x04 != 0 {
                let granule = u64::from_le_bytes(out[pos + 6..pos + 14].try_into().unwrap());
                assert_eq!(granule, 0xDEAD_BEEF, "audio granule position must be preserved");
                found_audio = true;
            }
            pos = end;
        }
        assert!(found_audio, "the EOS audio page must be present");
    }

    #[test]
    fn write_ogg_re_pages_a_comment_that_grows_across_a_page_boundary() {
        // A comment big enough to force the comment+setup region onto more than one page (segments > 255),
        // exercising the multi-page re-lacing + continuation-flag + page renumbering path.
        let big = "X".repeat(70_000);
        let (orig, _ident, audio_body) = build_ogg_vorbis(&[vfield("Title", "small")], 0x0BAD_F00D);

        let out = crate::media_meta::write_back(
            "ogg",
            &orig,
            &[crate::media_meta_edit::MetaEdit::Set { group: "vorbis".into(), key: "Comment".into(), value: big.clone() }],
        )
        .expect("ogg is writable");

        let after = crate::media_meta::read_all("ogg", &out);
        assert_eq!(get(&after, "Comment"), Some(big.as_str()));
        assert_eq!(get(&after, "Title"), Some("small"));
        let (valid, _pages) = all_page_crcs_valid(&out);
        assert!(valid, "every re-paged output page CRC must validate");
        assert!(contains_subslice(&out, &audio_body), "audio data must survive re-paging");

        // The crate's own reader reassembles the comment across the new page boundary.
        assert_eq!(crate::media_meta_read::read_ogg(&out).iter().find(|f| f.key == "Comment").map(|f| f.value.as_str()), Some(big.as_str()));
    }

    #[test]
    fn write_ogg_errs_on_non_ogg_and_incomplete_streams_without_panicking() {
        for orig in [&b""[..], &b"Ogg"[..], &b"fLaC...."[..], &b"just some bytes"[..], &b"OggS-too-short"[..]] {
            assert!(write_ogg(orig, &[vfield("Title", "X")]).is_err());
        }
        // A single BOS page carrying only the ident header (no comment/setup) → no complete header set → Err.
        let mut ident = OGG_IDENT_SIG.to_vec();
        ident.extend_from_slice(&[0u8; 22]);
        ident.push(0x01);
        let just_ident = build_page(0x02, 0, 7, 0, &lace_packet(&ident));
        assert!(write_ogg(&just_ident, &[vfield("Title", "X")]).is_err());
    }

    #[test]
    fn write_ogg_refuses_multiplexed_streams() {
        // Two logical streams (different serials) — refused rather than risk corrupting the other's paging.
        let (a, _, _) = build_ogg_vorbis(&[vfield("Title", "A")], 1);
        let (b, _, _) = build_ogg_vorbis(&[vfield("Title", "B")], 2);
        let mut muxed = a;
        muxed.extend_from_slice(&b);
        assert!(write_ogg(&muxed, &[vfield("Title", "X")]).is_err());
    }

    #[test]
    fn write_ogg_never_panics_on_any_truncation_of_a_valid_stream() {
        let (valid, _, _) = build_ogg_vorbis(&[vfield("Title", "T"), vfield("Artist", "A")], 0x4242);
        for cut in 0..valid.len() {
            let _ = write_ogg(&valid[..cut], &[vfield("Title", "X")]); // must never panic
        }
    }

    #[test]
    fn write_ogg_output_reparses_cleanly_through_the_crate_reader() {
        let (orig, _, _) = build_ogg_vorbis(&[vfield("Title", "Orig"), vfield("Artist", "Band")], 0x99);
        let out = write_ogg(
            &orig,
            &crate::media_meta_edit::apply_edits(
                &crate::media_meta::read_all("ogg", &orig),
                &[crate::media_meta_edit::MetaEdit::Set { group: "vorbis".into(), key: "Album".into(), value: "Fresh".into() }],
            )
            .fields,
        )
        .unwrap();
        let f = crate::media_meta_read::read_ogg(&out);
        assert_eq!(get(&f, "Album"), Some("Fresh"));
        assert_eq!(get(&f, "Title"), Some("Orig"));
        assert_eq!(get(&f, "Artist"), Some("Band"));
    }

    // ---- WAV / RIFF-INFO write-back (CPE-1298) ----

    fn wfield(key: &str, value: &str) -> MetaField {
        MetaField { group: "wav".to_string(), key: key.to_string(), value: value.to_string(), editable: true }
    }

    /// Serialize one raw RIFF chunk: 4-byte id + little-endian 4-byte size + data, padded to even.
    fn riff_chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut c = Vec::with_capacity(8 + data.len() + 1);
        c.extend_from_slice(id);
        c.extend_from_slice(&(data.len() as u32).to_le_bytes());
        c.extend_from_slice(data);
        if data.len() % 2 != 0 {
            c.push(0);
        }
        c
    }

    /// A `LIST`/`INFO` chunk built from `fields`, independently of [`build_wav_info`]'s caller — used to
    /// seed a fixture's *existing* INFO chunk.
    fn info_chunk(fields: &[MetaField]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"INFO");
        data.extend_from_slice(&build_wav_info(fields));
        riff_chunk(b"LIST", &data)
    }

    /// Build a minimal but valid WAV file: `RIFF`+size+`WAVE` header followed by the given already-
    /// serialized chunks (e.g. via [`riff_chunk`]/[`info_chunk`]).
    fn build_wav(chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        for c in chunks {
            body.extend_from_slice(c);
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    /// A representative 16-byte PCM `fmt ` chunk.
    fn fmt_chunk() -> Vec<u8> {
        let fmt_data: [u8; 16] = [1, 0, 2, 0, 0x44, 0xAC, 0, 0, 0x10, 0xB1, 0x02, 0, 4, 0, 16, 0];
        riff_chunk(b"fmt ", &fmt_data)
    }

    fn data_chunk(audio: &[u8]) -> Vec<u8> {
        riff_chunk(b"data", audio)
    }

    /// Read the outer `RIFF` size field out of a written WAV, for asserting it matches the true total.
    fn riff_size_field(bytes: &[u8]) -> usize {
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize
    }

    #[test]
    fn write_wav_round_trips_edits_and_preserves_fmt_and_data_byte_for_byte() {
        let fmt = fmt_chunk();
        let old_info = info_chunk(&[wfield("Title", "Old Title"), wfield("Artist", "Keep Artist")]);
        let audio = b"SOME-AUDIO-SAMPLE-BYTES".to_vec();
        let data = data_chunk(&audio);
        let orig = build_wav(&[fmt.clone(), old_info, data.clone()]);

        // Sanity: the reader sees the old tags before we touch anything.
        assert_eq!(get(&read_wav(&orig), "Title"), Some("Old Title"));

        let written = write_wav(&orig, &[wfield("Title", "New Title"), wfield("Artist", "Keep Artist")]).unwrap();

        // `fmt ` stayed in place (it precedes the replaced LIST/INFO chunk, so its byte range is untouched);
        // `data` is preserved byte-for-byte too (its position may shift if the INFO chunk's length changed).
        assert_eq!(&written[12..12 + fmt.len()], &fmt[..]);
        assert!(contains_subslice(&written, &data), "data chunk must be preserved byte-for-byte");

        // Outer RIFF size matches the true total.
        assert_eq!(riff_size_field(&written), written.len() - 8);

        let read_back = read_wav(&written);
        assert_eq!(get(&read_back, "Title"), Some("New Title"));
        assert_eq!(get(&read_back, "Artist"), Some("Keep Artist"));
    }

    #[test]
    fn write_wav_inserts_info_chunk_when_absent() {
        let fmt = fmt_chunk();
        let audio = b"AUDIO-NO-INFO-YET".to_vec();
        let data = data_chunk(&audio);
        let orig = build_wav(&[fmt.clone(), data.clone()]); // no LIST/INFO chunk at all

        assert!(read_wav(&orig).is_empty()); // sanity: nothing to read yet

        let written = write_wav(&orig, &[wfield("Title", "Fresh Tag")]).unwrap();

        assert_eq!(get(&read_wav(&written), "Title"), Some("Fresh Tag"));
        assert!(contains_subslice(&written, &fmt));
        assert!(contains_subslice(&written, &data));
        assert_eq!(riff_size_field(&written), written.len() - 8);
    }

    #[test]
    fn write_wav_does_not_confuse_a_non_info_list_chunk_with_the_info_chunk() {
        let fmt = fmt_chunk();
        let other_list = riff_chunk(b"LIST", b"adtlSOMEOTHERDATA"); // LIST chunk whose form type isn't INFO
        let audio = b"AUDIO".to_vec();
        let data = data_chunk(&audio);
        let orig = build_wav(&[fmt, other_list.clone(), data]);
        assert!(read_wav(&orig).is_empty()); // the reader doesn't treat it as INFO either

        let written = write_wav(&orig, &[wfield("Title", "New")]).unwrap();
        assert!(contains_subslice(&written, &other_list), "non-INFO LIST chunk must be preserved byte-for-byte");
        assert_eq!(get(&read_wav(&written), "Title"), Some("New"));
    }

    #[test]
    fn write_wav_writing_twice_is_idempotent_payload_wise() {
        let fmt = fmt_chunk();
        let audio = b"AUDIO-DATA-HERE".to_vec();
        let data = data_chunk(&audio);
        let orig = build_wav(&[fmt, data]);
        let fields = [wfield("Title", "A"), wfield("Artist", "B")];
        let once = write_wav(&orig, &fields).unwrap();
        let twice = write_wav(&once, &fields).unwrap();
        assert_eq!(once, twice, "re-writing over an already-written WAV must produce identical bytes");
    }

    #[test]
    fn write_wav_errs_on_non_riff_and_malformed_input_without_panicking() {
        for orig in [&b""[..], &b"RIFF"[..], &b"RIFFxxxxWAVEjunk"[..8], &b"not a wav file at all"[..], &[0xFFu8, 0xFB, 0x90, 0x00][..]] {
            assert!(write_wav(orig, &[wfield("Title", "X")]).is_err());
        }

        // Well-formed RIFF/WAVE header but a chunk that claims to run past EOF.
        let mut truncated = Vec::new();
        truncated.extend_from_slice(b"RIFF");
        truncated.extend_from_slice(&100u32.to_le_bytes());
        truncated.extend_from_slice(b"WAVE");
        truncated.extend_from_slice(b"fmt ");
        truncated.extend_from_slice(&999u32.to_le_bytes()); // claims 999 bytes, far more than present
        truncated.extend_from_slice(&[0u8; 4]);
        assert!(write_wav(&truncated, &[wfield("Title", "X")]).is_err());
    }

    #[test]
    fn write_wav_never_panics_on_any_truncation_of_a_valid_stream() {
        let fmt = fmt_chunk();
        let info = info_chunk(&[wfield("Title", "T")]);
        let audio = b"AUDIO-BYTES-HERE".to_vec();
        let data = data_chunk(&audio);
        let valid = build_wav(&[fmt, info, data]);
        for cut in 0..valid.len() {
            let _ = write_wav(&valid[..cut], &[wfield("Title", "X")]); // must never panic
        }
    }

    #[test]
    fn write_wav_skips_unmappable_keys_and_blank_values() {
        let fmt = fmt_chunk();
        let data = data_chunk(b"AUDIO");
        let orig = build_wav(&[fmt, data]);
        let fields = vec![
            wfield("Title", "Kept"),
            wfield("Some Random Thing", "value"), // no raw INFO id mapping — skipped
            wfield("Comment", ""),                // blank — skipped
            MetaField { group: "id3".into(), key: "Artist".into(), value: "Ignored".into(), editable: true }, // wrong group
        ];
        let written = write_wav(&orig, &fields).unwrap();
        let read_back = read_wav(&written);
        assert_eq!(read_back.len(), 1);
        assert_eq!(get(&read_back, "Title"), Some("Kept"));
    }
}
