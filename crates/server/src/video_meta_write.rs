//! MP4/MOV video-metadata **write codec** (CPE-1309, epic CPE-725): the write-back counterpart of
//! [`crate::video_meta_read::read_mp4`]. Writes the nine iTunes-style descriptive tags
//! (`©nam/©ART/©alb/©day/©cmt/©gen/©wrt/©too/cprt` → Title/Artist/Album/Year/Comment/Genre/Composer/
//! Encoder/Copyright, group `"video"`) into `moov ▸ udta ▸ meta ▸ ilst`, round-tripping through the reader.
//!
//! # The safety problem this codec is built around
//!
//! An MP4's sample tables (`stco` / `co64`, inside every `trak`'s `stbl`) hold **absolute file offsets**
//! into the media data (`mdat`). *Any* edit that shifts `mdat`'s position — growing a box that precedes
//! it, moving `moov` before it — silently invalidates every one of those offsets: the file still parses,
//! the tag still reads back, but playback desyncs. A naive "parse `moov`, re-serialize it, write the file"
//! writer does exactly that. So this codec **never moves `mdat` and never touches `stco`/`co64`.** It
//! mirrors [`crate::media_meta_write::write_pdf`]'s incremental-append strategy:
//!
//! 1. Byte-copy the whole original `moov` — every `trak`/`stbl`/`stco`/`co64` carried through **unparsed**.
//! 2. In the copy only, insert-or-replace the `udta ▸ meta ▸ ilst` iTunes atoms (synthesizing `udta`/`meta`/
//!    `ilst` if absent), recomputing sizes bottom-up `ilst → meta → udta → moov`; every sibling (`mvhd`,
//!    each `trak`) is preserved byte-for-byte, and any unknown `ilst` atom (cover art, `trkn`, …) survives.
//! 3. **Append** the rebuilt `moov` at true EOF.
//! 4. **Shadow** the original `moov` by overwriting only its 4-byte type `"moov"` → `"free"` in place (its
//!    size word is untouched, so it stays self-delimiting dead space). This is the *only* mutation to any
//!    pre-existing byte, and it doesn't change the file's length — so `mdat` keeps its exact absolute
//!    position and the verbatim-copied `stco`/`co64` offsets stay correct. Layout-agnostic: works for both
//!    faststart (`moov` before `mdat`) and trailing (`moov` after `mdat`) files.
//!
//! # Refusals (honest `Err`, never a guess)
//! - **Fragmented MP4** (a top-level `moof`/`mfra`): samples live in fragments with their own location
//!   model, not addressed by `moov`'s `stco`; re-homing tags there is out of scope.
//! - **A top-level `size == 0` box** (runs to EOF): appending after it would extend that box and swallow the
//!   new `moov`; we refuse rather than rewrite its size word.
//! - Truncated / malformed box structure.
//!
//! Pure + std-only (no new deps): a bounded hand-roll over [`crate::iso_bmff`]'s shared box primitives,
//! fully cargo-testable by round-tripping through [`crate::video_meta_read::read_mp4`]. Bounds-checked
//! throughout — malformed/short input returns `Err`, never panics.

use crate::iso_bmff::{push_box, raw_box_size32, read_box_header};
use crate::media_meta_edit::MetaField;

const COPYRIGHT_SIGN: u8 = 0xA9;

/// The nine iTunes-style tags the studio reads/writes for video, as `(friendly key, 4-byte atom type)`.
/// The inverse of [`crate::video_meta_read`]'s `friendly_key`. Canonical write order.
const VIDEO_TAGS: &[(&str, [u8; 4])] = &[
    ("Title", [COPYRIGHT_SIGN, b'n', b'a', b'm']),
    ("Artist", [COPYRIGHT_SIGN, b'A', b'R', b'T']),
    ("Album", [COPYRIGHT_SIGN, b'a', b'l', b'b']),
    ("Year", [COPYRIGHT_SIGN, b'd', b'a', b'y']),
    ("Comment", [COPYRIGHT_SIGN, b'c', b'm', b't']),
    ("Genre", [COPYRIGHT_SIGN, b'g', b'e', b'n']),
    ("Composer", [COPYRIGHT_SIGN, b'w', b'r', b't']),
    ("Encoder", [COPYRIGHT_SIGN, b't', b'o', b'o']),
    ("Copyright", *b"cprt"),
];

/// Whether a 4-byte `ilst` atom type is one of the nine this codec owns (and would therefore re-emit from
/// `fields`). Unknown atoms — cover art (`covr`), track number (`trkn`), binary genre (`gnre`), … — are
/// preserved verbatim rather than dropped.
fn is_known_tag(atom_type: &[u8; 4]) -> bool {
    VIDEO_TAGS.iter().any(|(_, cc)| cc == atom_type)
}

/// Write the nine iTunes-style tags in `fields` (`group == "video"`) into `orig`'s `moov/udta/meta/ilst`
/// via the never-move-`mdat` append-and-shadow strategy documented on this module. Returns the new file
/// bytes, or `Err` (never a panic) for a fragmented MP4, a top-level open-ended (`size == 0`) box, a file
/// with no `moov`, or truncated/malformed box structure.
pub fn write_mp4(orig: &[u8], fields: &[MetaField]) -> Result<Vec<u8>, String> {
    // Full top-level scan: locate `moov`, and refuse fragmented (`moof`/`mfra`) or open-ended (`size == 0`)
    // files anywhere at the top level — scanning the whole file, not just up to `moov`.
    let mut moov: Option<(usize, usize, usize)> = None; // (header_start, content_start, box_end)
    let mut offset = 0usize;
    while offset < orig.len() {
        if offset + 8 > orig.len() {
            return Err("truncated MP4 (incomplete top-level box header)".into());
        }
        let raw = raw_box_size32(orig, offset).ok_or("truncated MP4 (unreadable box size)")?;
        if raw == 0 {
            return Err("MP4 has a top-level open-ended (size==0) box; refusing to append safely".into());
        }
        let h = read_box_header(orig, offset, orig.len()).ok_or("malformed or truncated MP4 box structure")?;
        match &h.box_type {
            b"moof" | b"mfra" => {
                return Err("fragmented MP4 (moof/mfra) is not supported for metadata write-back".into());
            }
            b"moov" => moov = Some((h.header_start, h.content_start, h.box_end)),
            _ => {}
        }
        offset = h.box_end;
    }
    let (moov_hdr, moov_cs, moov_ce) = moov.ok_or("MP4 has no moov box to write metadata into")?;

    // Rebuild the moov's *content* (children) with udta/meta/ilst inserted-or-replaced.
    let new_moov_content = rebuild_moov(orig, moov_cs, moov_ce, fields)?;

    // Append the rebuilt moov at true EOF, then shadow the original moov's type in place. The type always
    // lives at header_start+4..+8 (both 32-bit and 64-bit-largesize encodings), and this write leaves the
    // file length unchanged, so mdat never moves and the copied stco/co64 offsets stay valid.
    let mut out = orig.to_vec();
    out[moov_hdr + 4..moov_hdr + 8].copy_from_slice(b"free");
    push_box(&mut out, b"moov", &new_moov_content);
    Ok(out)
}

/// Rebuild the `moov` box's content: copy every direct child verbatim except `udta`, which is rebuilt (or
/// synthesized) with the new `meta/ilst`. `udta` is emitted last (conventional, and order-independent for
/// the reader). `Err` if a child box is malformed.
fn rebuild_moov(bytes: &[u8], start: usize, end: usize, fields: &[MetaField]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut udta: Option<(usize, usize)> = None; // (content_start, box_end)
    let mut offset = start;
    while offset < end {
        let h = read_box_header(bytes, offset, end).ok_or("malformed moov child box")?;
        if &h.box_type == b"udta" {
            udta = Some((h.content_start, h.box_end));
        } else {
            out.extend_from_slice(&bytes[offset..h.box_end]); // mvhd, trak, … carried byte-for-byte
        }
        offset = h.box_end;
    }
    let new_udta = match udta {
        Some((cs, ce)) => rebuild_udta(bytes, cs, ce, fields)?,
        None => build_udta(fields),
    };
    push_box(&mut out, b"udta", &new_udta);
    Ok(out)
}

/// Rebuild the `udta` box's content: copy every direct child verbatim except `meta`, which is rebuilt (or
/// synthesized). `meta` is emitted last.
fn rebuild_udta(bytes: &[u8], start: usize, end: usize, fields: &[MetaField]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut meta: Option<(usize, usize)> = None;
    let mut offset = start;
    while offset < end {
        let h = read_box_header(bytes, offset, end).ok_or("malformed udta child box")?;
        if &h.box_type == b"meta" {
            meta = Some((h.content_start, h.box_end));
        } else {
            out.extend_from_slice(&bytes[offset..h.box_end]);
        }
        offset = h.box_end;
    }
    let new_meta = match meta {
        Some((cs, ce)) => rebuild_meta(bytes, cs, ce, fields)?,
        None => build_meta(fields),
    };
    push_box(&mut out, b"meta", &new_meta);
    Ok(out)
}

/// Rebuild the `meta` box's content. A `meta` box is (usually) a FullBox: an optional 4-byte version+flags
/// prelude before its child boxes — detected exactly as [`crate::video_meta_read`]'s reader does (does a
/// plausible box header start 4 bytes in?) so the rewrite round-trips through the same reader. The prelude
/// (if present) and every child except `ilst` are preserved verbatim; `ilst` is rebuilt (or synthesized)
/// and emitted last.
fn rebuild_meta(bytes: &[u8], start: usize, end: usize, fields: &[MetaField]) -> Result<Vec<u8>, String> {
    let has_prelude = start.checked_add(4).is_some_and(|s| s <= end && read_box_header(bytes, s, end).is_some());
    let children_start = if has_prelude { start + 4 } else { start };

    let mut out = Vec::new();
    if has_prelude {
        out.extend_from_slice(&bytes[start..start + 4]); // version+flags prelude
    }

    let mut ilst: Option<Vec<u8>> = None;
    let mut offset = children_start;
    while offset < end {
        let h = read_box_header(bytes, offset, end).ok_or("malformed meta child box")?;
        if &h.box_type == b"ilst" {
            ilst = Some(rebuild_ilst(bytes, h.content_start, h.box_end, fields)?);
        } else {
            out.extend_from_slice(&bytes[offset..h.box_end]); // hdlr, keys, … preserved
        }
        offset = h.box_end;
    }
    let ilst = ilst.unwrap_or_else(|| ilst_from_fields(fields));
    push_box(&mut out, b"ilst", &ilst);
    Ok(out)
}

/// Rebuild the `ilst` box's content: every **unknown** atom (cover art, `trkn`, `gnre`, …) is preserved
/// byte-for-byte; the nine **known** atoms are dropped here and re-emitted from `fields` in canonical order
/// (a tag absent from `fields` — cleared — is simply not re-emitted). `Err` if an atom is malformed.
fn rebuild_ilst(bytes: &[u8], start: usize, end: usize, fields: &[MetaField]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut offset = start;
    while offset < end {
        let h = read_box_header(bytes, offset, end).ok_or("malformed ilst atom")?;
        if !is_known_tag(&h.box_type) {
            out.extend_from_slice(&bytes[offset..h.box_end]);
        }
        offset = h.box_end;
    }
    append_known_atoms(&mut out, fields);
    Ok(out)
}

/// Build a fresh `ilst` content from just the nine known fields (no pre-existing atoms to preserve).
fn ilst_from_fields(fields: &[MetaField]) -> Vec<u8> {
    let mut out = Vec::new();
    append_known_atoms(&mut out, fields);
    out
}

/// Append, in canonical order, one iTunes atom per non-blank `group == "video"` field whose key is one of
/// the nine known tags. Each atom is `[size][4cc]` wrapping a `data` box: `[size]["data"][flags u32 = 1
/// (UTF-8 text)][reserved u32 = 0][UTF-8 value]` — exactly the shape [`crate::video_meta_read`]'s
/// `read_data_text` decodes.
fn append_known_atoms(out: &mut Vec<u8>, fields: &[MetaField]) {
    for (friendly, atom_type) in VIDEO_TAGS {
        let Some(field) =
            fields.iter().find(|f| f.group.eq_ignore_ascii_case("video") && f.key == *friendly && !f.value.is_empty())
        else {
            continue;
        };
        let mut data_content = Vec::with_capacity(8 + field.value.len());
        data_content.extend_from_slice(&1u32.to_be_bytes()); // version(0) + flags = UTF-8 text
        data_content.extend_from_slice(&0u32.to_be_bytes()); // reserved (locale/country+language)
        data_content.extend_from_slice(field.value.as_bytes());

        let mut atom_content = Vec::new();
        push_box(&mut atom_content, b"data", &data_content);
        push_box(out, atom_type, &atom_content);
    }
}

/// Build a fresh `udta` content wrapping a fresh `meta`.
fn build_udta(fields: &[MetaField]) -> Vec<u8> {
    let mut out = Vec::new();
    push_box(&mut out, b"meta", &build_meta(fields));
    out
}

/// Build a fresh `meta` content: the standard 4-byte version+flags prelude, an iTunes `hdlr` (`mdir`/`appl`)
/// so real players recognize the metadata, and an `ilst` of the known fields.
fn build_meta(fields: &[MetaField]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 0, 0]); // version+flags prelude
    push_box(&mut out, b"hdlr", &hdlr_mdir_content());
    push_box(&mut out, b"ilst", &ilst_from_fields(fields));
    out
}

/// The content of a minimal iTunes metadata `hdlr` box: version+flags, pre_defined, handler_type `mdir`,
/// the conventional `appl` reserved word + zero padding, and an empty NUL-terminated name.
fn hdlr_mdir_content() -> Vec<u8> {
    let mut c = Vec::with_capacity(25);
    c.extend_from_slice(&[0, 0, 0, 0]); // version + flags
    c.extend_from_slice(&[0, 0, 0, 0]); // pre_defined
    c.extend_from_slice(b"mdir"); // handler_type
    c.extend_from_slice(b"appl"); // reserved[0] (conventional for iTunes metadata)
    c.extend_from_slice(&[0, 0, 0, 0]); // reserved[1]
    c.extend_from_slice(&[0, 0, 0, 0]); // reserved[2]
    c.push(0); // name: empty, NUL-terminated
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::iso_bmff::find_child_box;
    use crate::video_meta_read::read_mp4;

    // ---- fixture builders ----------------------------------------------------------------------------

    fn box_(box_type: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&((8 + content.len()) as u32).to_be_bytes());
        b.extend_from_slice(box_type);
        b.extend_from_slice(content);
        b
    }

    fn data_box_text(text: &str) -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(&1u32.to_be_bytes());
        c.extend_from_slice(&0u32.to_be_bytes());
        c.extend_from_slice(text.as_bytes());
        box_(b"data", &c)
    }

    fn tag_atom(cc: &[u8; 4], text: &str) -> Vec<u8> {
        box_(cc, &data_box_text(text))
    }

    const NAM: [u8; 4] = [0xA9, b'n', b'a', b'm'];
    const ART: [u8; 4] = [0xA9, b'A', b'R', b'T'];

    const MDAT_MARKER: &[u8] = b"MARKER-SAMPLE-DATA";
    /// Byte offset of `MDAT_MARKER` within the `mdat` box *content* (after its 8-byte header).
    const MARKER_POS_IN_MDAT: usize = 6;

    fn video_field(key: &str, value: &str) -> MetaField {
        MetaField { group: "video".into(), key: key.into(), value: value.into(), editable: true }
    }

    /// A minimal but structurally real `trak`: `trak ▸ mdia ▸ minf ▸ stbl ▸ (stco|co64)` whose single chunk
    /// offset is `chunk_abs`. `wide` selects `co64` (u64 offsets) over `stco` (u32).
    fn trak_with_chunk_offset(chunk_abs: u64, wide: bool) -> Vec<u8> {
        let sample_table = if wide {
            let mut c = Vec::new();
            c.extend_from_slice(&0u32.to_be_bytes()); // version + flags
            c.extend_from_slice(&1u32.to_be_bytes()); // entry_count
            c.extend_from_slice(&chunk_abs.to_be_bytes()); // 64-bit chunk offset
            box_(b"co64", &c)
        } else {
            let mut c = Vec::new();
            c.extend_from_slice(&0u32.to_be_bytes()); // version + flags
            c.extend_from_slice(&1u32.to_be_bytes()); // entry_count
            c.extend_from_slice(&(chunk_abs as u32).to_be_bytes()); // 32-bit chunk offset
            box_(b"stco", &c)
        };
        let stbl = box_(b"stbl", &sample_table);
        let minf = box_(b"minf", &stbl);
        let mdia = box_(b"mdia", &minf);
        box_(b"trak", &mdia)
    }

    /// A version-0 `mvhd` (timescale 1000, duration 5000 → 5.0s) so the duration reader stays happy.
    fn mvhd() -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(&[0, 0, 0, 0]); // version 0 + flags
        c.extend_from_slice(&0u32.to_be_bytes()); // creation
        c.extend_from_slice(&0u32.to_be_bytes()); // modification
        c.extend_from_slice(&1000u32.to_be_bytes()); // timescale
        c.extend_from_slice(&5000u32.to_be_bytes()); // duration
        c.extend_from_slice(&[0u8; 8]); // padding
        box_(b"mvhd", &c)
    }

    /// Build a `moov` box (whole box, header included) with `mvhd`, a `trak` whose chunk offset is
    /// `chunk_abs`, and a `udta ▸ meta ▸ ilst` carrying the given atoms. `moov` length is independent of the
    /// *value* of `chunk_abs` (fixed-width field), which is what lets the caller compute offsets without
    /// circularity.
    fn moov_box(chunk_abs: u64, wide: bool, ilst_atoms: &[Vec<u8>]) -> Vec<u8> {
        let mut ilst_content = Vec::new();
        for a in ilst_atoms {
            ilst_content.extend_from_slice(a);
        }
        let ilst = box_(b"ilst", &ilst_content);
        let mut meta_content = Vec::new();
        meta_content.extend_from_slice(&[0, 0, 0, 0]); // meta prelude
        meta_content.extend_from_slice(&ilst);
        let meta = box_(b"meta", &meta_content);
        let udta = box_(b"udta", &meta);

        let mut moov_content = Vec::new();
        moov_content.extend_from_slice(&mvhd());
        moov_content.extend_from_slice(&trak_with_chunk_offset(chunk_abs, wide));
        moov_content.extend_from_slice(&udta);
        box_(b"moov", &moov_content)
    }

    fn mdat_box() -> Vec<u8> {
        let mut content = Vec::new();
        content.extend_from_slice(b"prefix"); // MARKER_POS_IN_MDAT == 6
        content.extend_from_slice(MDAT_MARKER);
        content.extend_from_slice(b"suffix");
        box_(b"mdat", &content)
    }

    /// Assemble a full MP4 (`ftyp` + `moov` + `mdat`, either order) whose `stco`/`co64` chunk offset points
    /// at `MDAT_MARKER`'s true absolute position. Returns `(file, marker_abs)`.
    fn build_mp4(moov_first: bool, wide: bool, ilst_atoms: &[Vec<u8>]) -> (Vec<u8>, u64) {
        let ftyp = box_(b"ftyp", b"isomiso2mp41");
        let mdat = mdat_box();

        let mdat_start = if moov_first {
            // moov length is independent of the chunk-offset value, so build a throwaway to measure it.
            let probe = moov_box(0, wide, ilst_atoms);
            ftyp.len() + probe.len()
        } else {
            ftyp.len()
        };
        let marker_abs = (mdat_start + 8 + MARKER_POS_IN_MDAT) as u64;
        let moov = moov_box(marker_abs, wide, ilst_atoms);

        let mut file = Vec::new();
        file.extend_from_slice(&ftyp);
        if moov_first {
            file.extend_from_slice(&moov);
            file.extend_from_slice(&mdat);
        } else {
            file.extend_from_slice(&mdat);
            file.extend_from_slice(&moov);
        }
        (file, marker_abs)
    }

    // ---- helpers for asserting on the output --------------------------------------------------------

    fn get<'a>(fields: &'a [MetaField], key: &str) -> Option<&'a str> {
        fields.iter().find(|f| f.key == key).map(|f| f.value.as_str())
    }

    /// Re-derive the chunk offsets from the **rewritten** file's live `moov` (the one that isn't `free`) by
    /// descending `moov ▸ trak ▸ mdia ▸ minf ▸ stbl ▸ (stco|co64)`, and return each absolute offset. This is
    /// the load-bearing guard: a writer that silently moved `mdat` would leave these pointing at the wrong
    /// bytes even though the tags still read back.
    fn derive_chunk_offsets(file: &[u8]) -> Vec<u64> {
        let (moov_cs, moov_ce) = find_child_box(file, 0, file.len(), b"moov").expect("live moov");
        let (trak_cs, trak_ce) = find_child_box(file, moov_cs, moov_ce, b"trak").expect("trak");
        let (mdia_cs, mdia_ce) = find_child_box(file, trak_cs, trak_ce, b"mdia").expect("mdia");
        let (minf_cs, minf_ce) = find_child_box(file, mdia_cs, mdia_ce, b"minf").expect("minf");
        let (stbl_cs, stbl_ce) = find_child_box(file, minf_cs, minf_ce, b"stbl").expect("stbl");

        if let Some((cs, _ce)) = find_child_box(file, stbl_cs, stbl_ce, b"stco") {
            let count = u32::from_be_bytes(file[cs + 4..cs + 8].try_into().unwrap()) as usize;
            (0..count).map(|i| u32::from_be_bytes(file[cs + 8 + i * 4..cs + 12 + i * 4].try_into().unwrap()) as u64).collect()
        } else if let Some((cs, _ce)) = find_child_box(file, stbl_cs, stbl_ce, b"co64") {
            let count = u32::from_be_bytes(file[cs + 4..cs + 8].try_into().unwrap()) as usize;
            (0..count).map(|i| u64::from_be_bytes(file[cs + 8 + i * 8..cs + 16 + i * 8].try_into().unwrap())).collect()
        } else {
            panic!("no stco/co64 in rewritten file");
        }
    }

    /// The original `moov`'s type has been shadowed to `free` iff the first top-level `moov`-typed box now
    /// appears only once (the appended one) and a `free` box of the original moov's size exists before it.
    fn count_top_level(file: &[u8], target: &[u8; 4]) -> usize {
        let mut n = 0;
        let mut off = 0;
        while off < file.len() {
            let h = read_box_header(file, off, file.len()).expect("well-formed top level");
            if &h.box_type == target {
                n += 1;
            }
            off = h.box_end;
        }
        n
    }

    // ---- the gate ------------------------------------------------------------------------------------

    /// The core round-trip + **offset-preservation** test, run in both layouts and both chunk-table widths.
    fn assert_round_trip_and_offsets(moov_first: bool, wide: bool) {
        let (orig, marker_abs) =
            build_mp4(moov_first, wide, &[tag_atom(&NAM, "Original Title"), tag_atom(&ART, "Keep Artist")]);

        // Sanity: the original stco/co64 points at the marker.
        assert_eq!(derive_chunk_offsets(&orig), vec![marker_abs]);
        assert_eq!(&orig[marker_abs as usize..marker_abs as usize + MDAT_MARKER.len()], MDAT_MARKER);

        // Edit only the Title; leave Artist untouched.
        let out = write_mp4(&orig, &[video_field("Title", "New Title"), video_field("Artist", "Keep Artist")])
            .expect("write must succeed on a well-formed single-mdat MP4");

        // (a) tag round-trips; (b) the untouched tag survives.
        let fields = read_mp4(&out);
        assert_eq!(get(&fields, "Title"), Some("New Title"), "edited tag must round-trip ({moov_first}/{wide})");
        assert_eq!(get(&fields, "Artist"), Some("Keep Artist"), "untouched tag must survive");

        // (c) the mdat marker is at its EXACT original absolute offset.
        assert_eq!(
            &out[marker_abs as usize..marker_abs as usize + MDAT_MARKER.len()],
            MDAT_MARKER,
            "mdat marker must not move"
        );

        // (d) LOAD-BEARING: re-derive stco/co64 from the REWRITTEN file and confirm each still dereferences to
        // the correct mdat marker bytes.
        let derived = derive_chunk_offsets(&out);
        assert_eq!(derived, vec![marker_abs], "rewritten chunk offsets must be unchanged");
        for off in derived {
            assert_eq!(
                &out[off as usize..off as usize + MDAT_MARKER.len()],
                MDAT_MARKER,
                "rewritten chunk offset must still deref to the sample data"
            );
        }

        // (e) the old moov's type is now `free` — exactly one live `moov` remains, and a `free` box appeared.
        assert_eq!(count_top_level(&out, b"moov"), 1, "old moov must be shadowed to free");
        assert_eq!(count_top_level(&out, b"free"), 1, "shadowed moov must appear as a free box");

        // (f) the output re-parses clean end-to-end: duration still reads, tags still read.
        assert_eq!(crate::video_column::video_cell(&out), crate::metadata_column::CellValue::Float(5.0));
        assert!(!read_mp4(&out).is_empty());
    }

    #[test]
    fn round_trips_and_preserves_offsets_moov_before_mdat_stco() {
        assert_round_trip_and_offsets(true, false);
    }

    #[test]
    fn round_trips_and_preserves_offsets_moov_after_mdat_stco() {
        assert_round_trip_and_offsets(false, false);
    }

    #[test]
    fn round_trips_and_preserves_offsets_moov_before_mdat_co64() {
        assert_round_trip_and_offsets(true, true);
    }

    #[test]
    fn round_trips_and_preserves_offsets_moov_after_mdat_co64() {
        assert_round_trip_and_offsets(false, true);
    }

    #[test]
    fn adds_a_new_tag_not_previously_present() {
        // Start with only a Title; add Album, Year, Copyright.
        let (orig, _) = build_mp4(true, false, &[tag_atom(&NAM, "Only Title")]);
        let out = write_mp4(
            &orig,
            &[
                video_field("Title", "Only Title"),
                video_field("Album", "An Album"),
                video_field("Year", "2026"),
                video_field("Copyright", "(c) 2026"),
            ],
        )
        .unwrap();
        let f = read_mp4(&out);
        assert_eq!(get(&f, "Title"), Some("Only Title"));
        assert_eq!(get(&f, "Album"), Some("An Album"));
        assert_eq!(get(&f, "Year"), Some("2026"));
        assert_eq!(get(&f, "Copyright"), Some("(c) 2026"));
    }

    #[test]
    fn clearing_a_tag_removes_it_on_reopen() {
        let (orig, _) = build_mp4(true, false, &[tag_atom(&NAM, "Title"), tag_atom(&ART, "Artist")]);
        // Pass only Title through (Artist cleared → absent from fields).
        let out = write_mp4(&orig, &[video_field("Title", "Title")]).unwrap();
        let f = read_mp4(&out);
        assert_eq!(get(&f, "Title"), Some("Title"));
        assert_eq!(get(&f, "Artist"), None, "cleared tag must be gone on reopen");
    }

    #[test]
    fn preserves_unknown_ilst_atoms() {
        // An unknown atom (cover-art stand-in) plus a Title. The unknown atom must survive verbatim.
        let covr = tag_atom(b"covr", "PRETEND-COVER-ART-BYTES");
        let (orig, _) = build_mp4(true, false, &[tag_atom(&NAM, "Title"), covr.clone()]);
        let out = write_mp4(&orig, &[video_field("Title", "New Title")]).unwrap();
        assert!(out.windows(covr.len()).any(|w| w == covr.as_slice()), "unknown ilst atom must be preserved");
        assert_eq!(get(&read_mp4(&out), "Title"), Some("New Title"));
    }

    #[test]
    fn synthesizes_udta_meta_ilst_when_absent() {
        // A moov with mvhd + trak but NO udta at all.
        let mut moov_content = Vec::new();
        moov_content.extend_from_slice(&mvhd());
        moov_content.extend_from_slice(&trak_with_chunk_offset(9999, false));
        let moov = box_(b"moov", &moov_content);
        let mut file = box_(b"ftyp", b"isom");
        file.extend_from_slice(&moov);
        file.extend_from_slice(&mdat_box());
        assert!(read_mp4(&file).is_empty()); // nothing to read yet

        let out = write_mp4(&file, &[video_field("Title", "Fresh"), video_field("Artist", "New")]).unwrap();
        let f = read_mp4(&out);
        assert_eq!(get(&f, "Title"), Some("Fresh"));
        assert_eq!(get(&f, "Artist"), Some("New"));
        // Duration (mvhd) still readable, i.e. the synthesized udta didn't disturb the copied children.
        assert_eq!(crate::video_column::video_cell(&out), crate::metadata_column::CellValue::Float(5.0));
    }

    #[test]
    fn refuses_fragmented_mp4() {
        // ftyp + moov + a top-level moof → fragmented.
        let (base, _) = build_mp4(true, false, &[tag_atom(&NAM, "T")]);
        let mut file = base;
        file.extend_from_slice(&box_(b"moof", b"fragment-run"));
        let err = write_mp4(&file, &[video_field("Title", "X")]).unwrap_err();
        assert!(err.contains("fragmented"), "got: {err}");
    }

    #[test]
    fn refuses_top_level_size_zero_box() {
        // ftyp + moov + an open-ended (size==0) mdat running to EOF.
        let (base, _) = build_mp4(true, false, &[tag_atom(&NAM, "T")]);
        let mut file = base;
        file.extend_from_slice(&0u32.to_be_bytes()); // size == 0
        file.extend_from_slice(b"mdat");
        file.extend_from_slice(b"open-ended-media-to-eof");
        let err = write_mp4(&file, &[video_field("Title", "X")]).unwrap_err();
        assert!(err.contains("size==0") || err.contains("open-ended"), "got: {err}");
    }

    #[test]
    fn errors_on_missing_moov_without_panicking() {
        let mut file = box_(b"ftyp", b"isom");
        file.extend_from_slice(&mdat_box());
        let err = write_mp4(&file, &[video_field("Title", "X")]).unwrap_err();
        assert!(err.contains("no moov"), "got: {err}");
    }

    #[test]
    fn truncated_input_never_panics() {
        let (file, _) = build_mp4(true, false, &[tag_atom(&NAM, "Title"), tag_atom(&ART, "Artist")]);
        for cut in 0..file.len() {
            // Must never panic on any truncation (Err is fine).
            let _ = write_mp4(&file[..cut], &[video_field("Title", "X")]);
        }
        // Pure garbage must not panic either.
        let garbage: Vec<u8> = (0..400u32).map(|i| (i.wrapping_mul(31) % 251) as u8).collect();
        let _ = write_mp4(&garbage, &[video_field("Title", "X")]);
    }

    #[test]
    fn output_is_original_bytes_plus_appended_moov_except_shadowed_type() {
        // The append-and-shadow invariant: output length == original + appended moov, and the only mutated
        // pre-existing bytes are the 4 that turned "moov" into "free".
        let (orig, _) = build_mp4(false, false, &[tag_atom(&NAM, "T")]);
        let out = write_mp4(&orig, &[video_field("Title", "T2")]).unwrap();
        assert!(out.len() > orig.len(), "output must have grown by the appended moov");
        // Everything before the original EOF equals the original except inside the (now-free) moov header.
        let mut differing = 0usize;
        for i in 0..orig.len() {
            if out[i] != orig[i] {
                differing += 1;
            }
        }
        assert_eq!(differing, 4, "exactly the 4 type bytes moov→free may differ in the original region");
    }
}
