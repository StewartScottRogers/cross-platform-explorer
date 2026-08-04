//! MP4/MOV video metadata **read codec** (CPE-1037, epic CPE-725): the video-family counterpart of
//! [`crate::media_meta_read::read_id3v2`] (audio) and [`crate::media_meta_read::read_exif`] (image).
//!
//! MP4/MOV files are **ISO-BMFF**: a flat, potentially-nested sequence of *boxes*, each a big-endian
//! `u32` size + 4-byte ASCII type (with an optional 64-bit largesize when `size == 1`, or "runs to end of
//! buffer" when `size == 0`). iTunes-style descriptive tags (title/artist/album/…) live nested at
//! `moov ▸ udta ▸ meta ▸ ilst`, where each `ilst` child box's own 4-byte type IS the tag key (e.g. `©nam`
//! for Title) and wraps a `data` sub-box carrying the actual value.
//!
//! [`crate::video_column`] already walks `moov/mvhd` for duration; this module walks the sibling
//! `moov/udta/meta/ilst` path for descriptive tags, surfacing them as [`MetaField`]s in group `"video"`
//! (all `editable: true` since CPE-1309 landed the [`crate::video_meta_write`] write-back codec). Pure
//! std, no new dependencies; every box size is clamped to the remaining buffer and nesting is depth-capped,
//! so malformed/truncated/adversarial input yields an empty (or partial) result and **never panics**. The
//! box-header/child primitives live in the shared [`crate::iso_bmff`] module.

use crate::iso_bmff::{find_child_box, read_box_header, BoxHeader};
use crate::media_meta_edit::MetaField;

/// Cap on box-tree nesting depth while descending to `ilst` — well beyond any real file's structure,
/// just a backstop against pathological/adversarial input.
const MAX_DEPTH: u32 = 16;

/// Read iTunes-style MP4/MOV metadata (`moov ▸ udta ▸ meta ▸ ilst`) from `bytes` into [`MetaField`]s in
/// group `"video"` (all `editable: true`). Returns an empty vec when the file isn't parseable ISO-BMFF,
/// has no `moov`, has `moov` but no `udta`/`meta`/`ilst`, or is truncated/garbage — never panics.
pub fn read_mp4(bytes: &[u8]) -> Vec<MetaField> {
    let Some((moov_start, moov_end)) = find_child_box(bytes, 0, bytes.len(), b"moov") else {
        return Vec::new();
    };
    let Some((udta_start, udta_end)) = find_child_box(bytes, moov_start, moov_end, b"udta") else {
        return Vec::new();
    };
    let Some((meta_start, meta_end)) = find_child_box(bytes, udta_start, udta_end, b"meta") else {
        return Vec::new();
    };

    // The `meta` box has a 4-byte version+flags prelude before its child boxes (unlike a plain container
    // box). Try skipping it; if what follows doesn't look like a valid box header, retry without the skip
    // — some writers omit the prelude.
    let ilst_range = find_ilst_in_meta(bytes, meta_start, meta_end);
    let Some((ilst_start, ilst_end)) = ilst_range else {
        return Vec::new();
    };

    parse_ilst(bytes, ilst_start, ilst_end, 0)
}

/// Locate `ilst` inside a `meta` box's content, defensively handling the optional 4-byte version+flags
/// prelude: try skipping it first (the common case), and if that doesn't yield a plausible box header,
/// retry treating the content as starting with boxes directly.
fn find_ilst_in_meta(bytes: &[u8], meta_start: usize, meta_end: usize) -> Option<(usize, usize)> {
    let skipped_start = meta_start.checked_add(4)?;
    if skipped_start <= meta_end && looks_like_box(bytes, skipped_start, meta_end) {
        if let Some(range) = find_child_box(bytes, skipped_start, meta_end, b"ilst") {
            return Some(range);
        }
    }
    // Retry without skipping the prelude (some writers omit it entirely).
    find_child_box(bytes, meta_start, meta_end, b"ilst")
}

/// Heuristic: does a plausible ISO-BMFF box header start at `offset`? Used only to decide whether the
/// `meta` box's 4-byte prelude should be skipped — a real box's size, added to `offset`, must land at or
/// before `end`, and the size must be nonzero (an empty box here would be nonsensical this early).
fn looks_like_box(bytes: &[u8], offset: usize, end: usize) -> bool {
    read_box_header(bytes, offset, end).is_some()
}

/// Walk `ilst`'s direct children — each one's own box type is the tag key — decoding any with a `data`
/// sub-box carrying UTF-8 text. Unknown/binary atoms are skipped. `depth` guards against pathological
/// nesting (capped at [`MAX_DEPTH`], though `ilst` children are normally flat).
fn parse_ilst(bytes: &[u8], start: usize, end: usize, depth: u32) -> Vec<MetaField> {
    let mut fields = Vec::new();
    if depth > MAX_DEPTH {
        return fields;
    }
    let mut offset = start;
    while offset < end {
        let Some(header) = read_box_header(bytes, offset, end) else { break };
        if let Some((key, value)) = decode_ilst_entry(bytes, &header) {
            if !value.is_empty() {
                fields.push(MetaField { group: "video".to_string(), key, value, editable: true });
            }
        }
        offset = header.box_end;
    }
    fields
}

/// Decode one `ilst` child box into `(friendly_key, text_value)`, or `None` if its tag is unrecognised or
/// it has no usable text `data` sub-box.
fn decode_ilst_entry(bytes: &[u8], entry: &BoxHeader) -> Option<(String, String)> {
    let key = friendly_key(&entry.box_type)?;
    let value = read_data_text(bytes, entry.content_start, entry.box_end)?;
    Some((key, value))
}

/// Find this atom's `data` sub-box and, if its type-flags low byte indicates UTF-8 text (`1`), decode the
/// payload as a `String`. Layout: `size(4)+type(4)="data"` + `version+flags(4)` + `reserved(4)` +
/// payload. Bounds-checked throughout; `None` on truncation, a non-`data` child, or a non-text flag.
fn read_data_text(bytes: &[u8], start: usize, end: usize) -> Option<String> {
    let (data_start, data_end) = find_child_box(bytes, start, end, b"data")?;
    // data_start..data_end is the `data` box's payload (after its own 8-byte header): version+flags(4) +
    // reserved(4) + text payload.
    if data_start.checked_add(8)? > data_end {
        return None;
    }
    let flags = u32::from_be_bytes(bytes.get(data_start..data_start + 4)?.try_into().ok()?);
    let type_flags = flags & 0x00FF_FFFF; // low 3 bytes carry the well-known-type indicator
    if type_flags != 1 {
        return None; // not UTF-8 text (e.g. binary gnre/trkn) — best-effort skip
    }
    let payload = bytes.get(data_start + 8..data_end)?;
    let text = std::str::from_utf8(payload).ok()?.trim_matches('\u{0}').trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Map an `ilst` atom's raw 4-byte type to a friendly key. `©` is encoded as byte `0xA9`. Unknown atoms
/// return `None` (skipped) rather than surfacing raw/binary tags.
fn friendly_key(box_type: &[u8; 4]) -> Option<String> {
    const COPYRIGHT_SIGN: u8 = 0xA9;
    let named = match box_type {
        [COPYRIGHT_SIGN, b'n', b'a', b'm'] => "Title",
        [COPYRIGHT_SIGN, b'A', b'R', b'T'] => "Artist",
        [COPYRIGHT_SIGN, b'a', b'l', b'b'] => "Album",
        [COPYRIGHT_SIGN, b'd', b'a', b'y'] => "Year",
        [COPYRIGHT_SIGN, b'c', b'm', b't'] => "Comment",
        [COPYRIGHT_SIGN, b'g', b'e', b'n'] => "Genre",
        [COPYRIGHT_SIGN, b'w', b'r', b't'] => "Composer",
        [COPYRIGHT_SIGN, b't', b'o', b'o'] => "Encoder",
        b"cprt" => "Copyright",
        _ => return None,
    };
    Some(named.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrap `content` in a box of the given 4-byte `type` (32-bit size — plenty for these fixtures).
    fn make_box(box_type: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        let total = (8 + content.len()) as u32;
        b.extend_from_slice(&total.to_be_bytes());
        b.extend_from_slice(box_type);
        b.extend_from_slice(content);
        b
    }

    /// Build a `data` box wrapping UTF-8 `text` with the "well-known type = UTF-8 text" flag (1).
    fn data_box_text(text: &str) -> Vec<u8> {
        let mut content = Vec::new();
        content.extend_from_slice(&1u32.to_be_bytes()); // version(0) + flags(1) = UTF-8 text
        content.extend_from_slice(&0u32.to_be_bytes()); // reserved
        content.extend_from_slice(text.as_bytes());
        make_box(b"data", &content)
    }

    /// Build a `data` box with a non-text well-known type (e.g. binary `gnre`/`trkn`-style payload).
    fn data_box_binary(payload: &[u8]) -> Vec<u8> {
        let mut content = Vec::new();
        content.extend_from_slice(&0u32.to_be_bytes()); // version(0) + flags(0) = binary/implicit
        content.extend_from_slice(&0u32.to_be_bytes()); // reserved
        content.extend_from_slice(payload);
        make_box(b"data", &content)
    }

    /// Build one `ilst` child atom (`©nam`, `©ART`, etc.) wrapping a `data` box.
    fn ilst_atom(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        make_box(tag, data)
    }

    const NAM: [u8; 4] = [0xA9, b'n', b'a', b'm'];
    const ART: [u8; 4] = [0xA9, b'A', b'R', b'T'];
    const ALB: [u8; 4] = [0xA9, b'a', b'l', b'b'];
    const DAY: [u8; 4] = [0xA9, b'd', b'a', b'y'];
    const CMT: [u8; 4] = [0xA9, b'c', b'm', b't'];
    const GEN: [u8; 4] = [0xA9, b'g', b'e', b'n'];
    const WRT: [u8; 4] = [0xA9, b'w', b'r', b't'];
    const TOO: [u8; 4] = [0xA9, b't', b'o', b'o'];

    /// Build a full minimal MP4: `ftyp` + `moov` > `udta` > `meta` (with the 4-byte version+flags
    /// prelude) > `ilst` containing the given already-built atoms.
    fn synthetic_mp4(ilst_atoms: &[Vec<u8>]) -> Vec<u8> {
        let mut ilst_content = Vec::new();
        for atom in ilst_atoms {
            ilst_content.extend_from_slice(atom);
        }
        let ilst = make_box(b"ilst", &ilst_content);

        let mut meta_content = Vec::new();
        meta_content.extend_from_slice(&[0, 0, 0, 0]); // meta's version+flags prelude
        meta_content.extend_from_slice(&ilst);
        let meta = make_box(b"meta", &meta_content);

        let udta = make_box(b"udta", &meta);
        let moov = make_box(b"moov", &udta);

        let mut f = Vec::new();
        f.extend_from_slice(&make_box(b"ftyp", b"isom\0\0\x02\0isomiso2mp41"));
        f.extend_from_slice(&moov);
        f
    }

    /// Same as [`synthetic_mp4`] but omits the `meta` prelude (some writers do), to exercise the
    /// no-prelude retry path.
    fn synthetic_mp4_no_meta_prelude(ilst_atoms: &[Vec<u8>]) -> Vec<u8> {
        let mut ilst_content = Vec::new();
        for atom in ilst_atoms {
            ilst_content.extend_from_slice(atom);
        }
        let ilst = make_box(b"ilst", &ilst_content);
        let meta = make_box(b"meta", &ilst); // no 4-byte prelude
        let udta = make_box(b"udta", &meta);
        let moov = make_box(b"moov", &udta);

        let mut f = Vec::new();
        f.extend_from_slice(&make_box(b"ftyp", b"isom"));
        f.extend_from_slice(&moov);
        f
    }

    fn get<'a>(fields: &'a [MetaField], key: &str) -> Option<&'a str> {
        fields.iter().find(|f| f.key == key).map(|f| f.value.as_str())
    }

    #[test]
    fn reads_title_and_artist_from_nested_ilst() {
        let file = synthetic_mp4(&[
            ilst_atom(&NAM, &data_box_text("Big Buck Bunny")),
            ilst_atom(&ART, &data_box_text("Blender Foundation")),
        ]);
        let f = read_mp4(&file);
        assert_eq!(get(&f, "Title"), Some("Big Buck Bunny"));
        assert_eq!(get(&f, "Artist"), Some("Blender Foundation"));
        assert!(f.iter().all(|x| x.group == "video" && x.editable));
    }

    #[test]
    fn reads_all_friendly_tags() {
        let file = synthetic_mp4(&[
            ilst_atom(&NAM, &data_box_text("T")),
            ilst_atom(&ART, &data_box_text("A")),
            ilst_atom(&ALB, &data_box_text("Al")),
            ilst_atom(&DAY, &data_box_text("2024")),
            ilst_atom(&CMT, &data_box_text("A comment")),
            ilst_atom(&GEN, &data_box_text("Documentary")),
            ilst_atom(&WRT, &data_box_text("A composer")),
            ilst_atom(&TOO, &data_box_text("HandBrake")),
            ilst_atom(b"cprt", &data_box_text("(c) 2024")),
        ]);
        let f = read_mp4(&file);
        assert_eq!(get(&f, "Title"), Some("T"));
        assert_eq!(get(&f, "Artist"), Some("A"));
        assert_eq!(get(&f, "Album"), Some("Al"));
        assert_eq!(get(&f, "Year"), Some("2024"));
        assert_eq!(get(&f, "Comment"), Some("A comment"));
        assert_eq!(get(&f, "Genre"), Some("Documentary"));
        assert_eq!(get(&f, "Composer"), Some("A composer"));
        assert_eq!(get(&f, "Encoder"), Some("HandBrake"));
        assert_eq!(get(&f, "Copyright"), Some("(c) 2024"));
    }

    #[test]
    fn handles_meta_without_prelude() {
        let file = synthetic_mp4_no_meta_prelude(&[ilst_atom(&NAM, &data_box_text("No Prelude Title"))]);
        let f = read_mp4(&file);
        assert_eq!(get(&f, "Title"), Some("No Prelude Title"));
    }

    #[test]
    fn skips_unknown_atoms_and_binary_data() {
        let file = synthetic_mp4(&[
            ilst_atom(&NAM, &data_box_text("Known Title")),
            ilst_atom(b"trkn", &data_box_binary(&[0, 0, 0, 1, 0, 0, 0, 0])), // binary track number
            ilst_atom(b"gnre", &data_box_binary(&[0, 12])),                 // binary genre id
            ilst_atom(b"xxxx", &data_box_text("Unrecognised tag")),         // unknown 4cc
        ]);
        let f = read_mp4(&file);
        assert_eq!(get(&f, "Title"), Some("Known Title"));
        assert_eq!(f.len(), 1); // binary + unknown atoms all skipped
    }

    #[test]
    fn empty_values_are_dropped() {
        let file = synthetic_mp4(&[
            ilst_atom(&NAM, &data_box_text("")),
            ilst_atom(&ART, &data_box_text("Real Artist")),
        ]);
        let f = read_mp4(&file);
        assert_eq!(get(&f, "Title"), None);
        assert_eq!(get(&f, "Artist"), Some("Real Artist"));
    }

    #[test]
    fn non_mp4_bytes_yield_empty() {
        assert!(read_mp4(b"").is_empty());
        assert!(read_mp4(b"not an mp4 file at all, just text").is_empty());
        assert!(read_mp4(b"ID3\x03\0\0\0\0\0\0").is_empty()); // a real ID3 tag, but not ISO-BMFF
    }

    #[test]
    fn moov_without_udta_yields_empty() {
        let mut f = Vec::new();
        f.extend_from_slice(&make_box(b"ftyp", b"isom"));
        f.extend_from_slice(&make_box(b"moov", &make_box(b"trak", b"whatever")));
        assert!(read_mp4(&f).is_empty());
    }

    #[test]
    fn no_moov_at_all_yields_empty() {
        let mut f = Vec::new();
        f.extend_from_slice(&make_box(b"ftyp", b"isom"));
        assert!(read_mp4(&f).is_empty());
    }

    #[test]
    fn udta_without_meta_or_meta_without_ilst_yields_empty() {
        let mut f = Vec::new();
        f.extend_from_slice(&make_box(b"ftyp", b"isom"));
        let udta_no_meta = make_box(b"udta", &make_box(b"free", b""));
        f.extend_from_slice(&make_box(b"moov", &udta_no_meta));
        assert!(read_mp4(&f).is_empty());

        let mut f2 = Vec::new();
        f2.extend_from_slice(&make_box(b"ftyp", b"isom"));
        let mut meta_content = Vec::new();
        meta_content.extend_from_slice(&[0, 0, 0, 0]);
        meta_content.extend_from_slice(&make_box(b"hdlr", b"mdirappl"));
        let meta_no_ilst = make_box(b"meta", &meta_content);
        let udta = make_box(b"udta", &meta_no_ilst);
        f2.extend_from_slice(&make_box(b"moov", &udta));
        assert!(read_mp4(&f2).is_empty());
    }

    #[test]
    fn declared_size_larger_than_available_bytes_yields_empty() {
        // A moov box that claims a size far beyond what's actually present.
        let mut f = Vec::new();
        f.extend_from_slice(&make_box(b"ftyp", b"isom"));
        f.extend_from_slice(&1_000_000u32.to_be_bytes());
        f.extend_from_slice(b"moov");
        f.extend_from_slice(&make_box(b"udta", &make_box(b"meta", b"")));
        assert!(read_mp4(&f).is_empty());
    }

    #[test]
    fn truncated_or_garbage_bytes_never_panic() {
        let file = synthetic_mp4(&[
            ilst_atom(&NAM, &data_box_text("Truncation Target")),
            ilst_atom(&ART, &data_box_text("Another Artist")),
        ]);
        for cut in 0..file.len() {
            let _ = read_mp4(&file[..cut]); // must never panic on any truncation
        }
        // Pure garbage / random-ish bytes must never panic either.
        let garbage: Vec<u8> = (0..300u32).map(|i| (i.wrapping_mul(37) % 251) as u8).collect();
        let _ = read_mp4(&garbage);
        assert!(read_mp4(&garbage[..8]).is_empty());
    }

    #[test]
    fn zero_size_box_running_to_end_is_handled() {
        // A moov box using size==0 ("runs to end of buffer") wrapping udta/meta/ilst with a title.
        let ilst = make_box(b"ilst", &ilst_atom(&NAM, &data_box_text("Zero Size Title")));
        let mut meta_content = Vec::new();
        meta_content.extend_from_slice(&[0, 0, 0, 0]);
        meta_content.extend_from_slice(&ilst);
        let meta = make_box(b"meta", &meta_content);
        let udta = make_box(b"udta", &meta);

        let mut f = Vec::new();
        f.extend_from_slice(&make_box(b"ftyp", b"isom"));
        // moov header with size 0, running to end of buffer.
        f.extend_from_slice(&0u32.to_be_bytes());
        f.extend_from_slice(b"moov");
        f.extend_from_slice(&udta);

        let fields = read_mp4(&f);
        assert_eq!(get(&fields, "Title"), Some("Zero Size Title"));
    }
}
