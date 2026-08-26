//! Camera-RAW embedded-preview extraction (CPE-1346, epic "Preview/edit support"). CR2/NEF/ARW are TIFF
//! containers wrapped around undemosaiced sensor data; rather than demosaic that (out of scope — a much
//! bigger job), every one of these formats already carries at least one embedded JPEG preview/thumbnail
//! that a normal JPEG decoder can show as-is. This module is a hand-rolled, bounds-checked TIFF/IFD
//! walker — **zero new dependency**: only `std`, plus `kamadak-exif` and `base64`, both already crate
//! deps (see `image_preview.rs`, which this mirrors the data-URL return style of).
//!
//! Approach: parse the 8-byte TIFF header (byte order + IFD0 offset), then recursively walk IFDs —
//! following both the NextIFD chain and pointer tags (`SubIFDs` 0x014A, `ExifIFD` 0x8769) — collecting
//! every embedded-JPEG candidate found via the `JPEGInterchangeFormat`/`Length` tag pair (0x0201/0x0202)
//! or a `Compression` 6/7 strip whose bytes start with the JPEG SOI marker. The **largest** candidate by
//! byte length is returned as-is (no re-encode) as a `data:image/jpeg;base64,...` URL. If the walk finds
//! nothing, falls back to `kamadak-exif`'s standard `In::THUMBNAIL` tag. Every offset/length is bounds-
//! checked before slicing; a cycle guard (visited-offset set) plus a hard cap on total IFDs visited means
//! a malformed, truncated, or deliberately cyclic file returns `Err`, never panics or hangs.

use std::collections::HashSet;
use std::fs;

/// Hard cap on total IFDs visited across the whole walk (NextIFD chain + SubIFDs/ExifIFD recursion).
/// Real raw files have a handful of IFDs (IFD0, one or two SubIFDs, ExifIFD, maybe a thumbnail IFD); 64
/// is generous headroom while still bounding a cyclic or IFD-bomb file to a bounded amount of work.
const MAX_IFDS_VISITED: usize = 64;

/// Cap on how many pointer values a single `SubIFDs`-style entry can contribute to the walk. A legitimate
/// file has at most a few SubIFDs; an entry claiming a huge `count` (attacker-controlled) must not turn
/// into an unbounded loop even though each individual lookup is bounds-checked and cheap.
const MAX_POINTERS_PER_ENTRY: u32 = 16;

const TAG_COMPRESSION: u16 = 0x0103;
const TAG_STRIP_OFFSETS: u16 = 0x0111;
const TAG_STRIP_BYTE_COUNTS: u16 = 0x0117;
const TAG_JPEG_OFFSET: u16 = 0x0201;
const TAG_JPEG_LENGTH: u16 = 0x0202;
const TAG_SUB_IFDS: u16 = 0x014A;
const TAG_EXIF_IFD: u16 = 0x8769;

const JPEG_SOI: [u8; 2] = [0xFF, 0xD8];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ByteOrder {
    Little,
    Big,
}

impl ByteOrder {
    fn u16(self, b: &[u8]) -> u16 {
        let a: [u8; 2] = [b[0], b[1]];
        match self {
            ByteOrder::Little => u16::from_le_bytes(a),
            ByteOrder::Big => u16::from_be_bytes(a),
        }
    }

    fn u32(self, b: &[u8]) -> u32 {
        let a: [u8; 4] = [b[0], b[1], b[2], b[3]];
        match self {
            ByteOrder::Little => u32::from_le_bytes(a),
            ByteOrder::Big => u32::from_be_bytes(a),
        }
    }
}

/// One raw 12-byte TIFF directory entry: tag, field type, value count, and the 4-byte value/offset slot
/// (interpreted differently depending on whether the whole value fits inline).
struct Entry {
    tag: u16,
    typ: u16,
    count: u32,
    raw: [u8; 4],
}

impl Entry {
    /// Byte size of one value of this entry's field type; `None` for a type this module never needs to
    /// read the value of (RATIONAL/DOUBLE and friends — the tags we care about are all SHORT/LONG).
    fn type_size(&self) -> Option<usize> {
        match self.typ {
            1 | 2 | 6 | 7 => Some(1),  // BYTE, ASCII, SBYTE, UNDEFINED
            3 | 8 => Some(2),          // SHORT, SSHORT
            4 | 9 | 11 => Some(4),     // LONG, SLONG, FLOAT
            _ => None,
        }
    }

    /// The `index`-th scalar value of this entry, read inline from `raw` when the whole value fits in
    /// the 4-byte slot, else from `buf` at the offset `raw` encodes. Bounds-checked throughout; `None` on
    /// an out-of-range index, an unsupported type, or a slice that would fall outside `buf`.
    fn value_u32(&self, buf: &[u8], bo: ByteOrder, index: usize) -> Option<u32> {
        if index >= self.count as usize {
            return None;
        }
        let size = self.type_size()?;
        let total = size.checked_mul(self.count as usize)?;
        if total <= 4 {
            let start = index.checked_mul(size)?;
            let end = start.checked_add(size)?;
            let bytes = self.raw.get(start..end)?;
            Some(Self::read(bytes, bo, size))
        } else {
            let base = bo.u32(&self.raw) as usize;
            let start = base.checked_add(index.checked_mul(size)?)?;
            let end = start.checked_add(size)?;
            let bytes = buf.get(start..end)?;
            Some(Self::read(bytes, bo, size))
        }
    }

    fn read(bytes: &[u8], bo: ByteOrder, size: usize) -> u32 {
        match size {
            1 => bytes[0] as u32,
            2 => bo.u16(bytes) as u32,
            _ => bo.u32(bytes),
        }
    }

    fn first_u32(&self, buf: &[u8], bo: ByteOrder) -> Option<u32> {
        self.value_u32(buf, bo, 0)
    }
}

struct Ifd {
    entries: Vec<Entry>,
    next: u32,
}

/// Read one IFD at `offset`: a 2-byte entry count, `count` × 12-byte entries, then a 4-byte next-IFD
/// offset. Every byte range is checked against `buf.len()` before slicing — a truncated or bogus offset
/// yields `Err`, never a panic.
fn read_ifd(buf: &[u8], bo: ByteOrder, offset: usize) -> Result<Ifd, String> {
    let count_bytes = buf.get(offset..offset.checked_add(2).ok_or("IFD offset overflow")?)
        .ok_or("IFD entry-count out of range")?;
    let count = bo.u16(count_bytes) as usize;

    let entries_start = offset + 2;
    let entries_len = count.checked_mul(12).ok_or("IFD entry count overflow")?;
    let entries_end = entries_start.checked_add(entries_len).ok_or("IFD entries out of range")?;
    let next_end = entries_end.checked_add(4).ok_or("IFD next-offset overflow")?;
    if next_end > buf.len() {
        return Err("IFD extends past end of buffer".to_string());
    }

    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let e_start = entries_start + i * 12;
        let e = &buf[e_start..e_start + 12];
        let mut raw = [0u8; 4];
        raw.copy_from_slice(&e[8..12]);
        entries.push(Entry { tag: bo.u16(&e[0..2]), typ: bo.u16(&e[2..4]), count: bo.u32(&e[4..8]), raw });
    }

    let next = bo.u32(&buf[entries_end..next_end]);
    Ok(Ifd { entries, next })
}

struct Candidate {
    offset: usize,
    len: usize,
}

/// Validate and record a candidate embedded JPEG at `offset`/`len`: bounds-checked, and only kept if the
/// bytes there actually start with the JPEG SOI marker (`FF D8`) — a bogus offset/length pair is silently
/// skipped rather than treated as an error, since it just means "not a real candidate."
fn push_candidate(buf: &[u8], offset: u32, len: u32, candidates: &mut Vec<Candidate>) {
    let offset = offset as usize;
    let len = len as usize;
    if len < JPEG_SOI.len() {
        return;
    }
    let Some(end) = offset.checked_add(len) else { return };
    let Some(bytes) = buf.get(offset..end) else { return };
    if bytes.starts_with(&JPEG_SOI) {
        candidates.push(Candidate { offset, len });
    }
}

/// Collect every embedded-JPEG candidate visible directly in this one IFD's entries (does not recurse).
fn collect_candidates(buf: &[u8], bo: ByteOrder, entries: &[Entry], candidates: &mut Vec<Candidate>) {
    let find = |tag: u16| entries.iter().find(|e| e.tag == tag);

    // (a) The standard "old-style JPEG" embedded-preview tag pair.
    if let (Some(off), Some(len)) = (find(TAG_JPEG_OFFSET), find(TAG_JPEG_LENGTH)) {
        if let (Some(off), Some(len)) = (off.first_u32(buf, bo), len.first_u32(buf, bo)) {
            push_candidate(buf, off, len, candidates);
        }
    }

    // (b) Compression 6/7 (old/new-style JPEG) backed by a strip, when the strip data itself starts with
    // a JPEG SOI marker — some raw formats store a JPEG blob under the strip tags instead of (a).
    if let Some(compression) = find(TAG_COMPRESSION).and_then(|e| e.first_u32(buf, bo)) {
        if compression == 6 || compression == 7 {
            if let (Some(off), Some(len)) = (find(TAG_STRIP_OFFSETS), find(TAG_STRIP_BYTE_COUNTS)) {
                if let (Some(off), Some(len)) = (off.first_u32(buf, bo), len.first_u32(buf, bo)) {
                    push_candidate(buf, off, len, candidates);
                }
            }
        }
    }
}

/// Recursively walk the IFD chain starting at `start` (following NextIFD), and for every `SubIFDs`
/// (0x014A) / `ExifIFD` (0x8769) pointer entry, recurse into that IFD too. `visited` + `visited_count`
/// are shared across the whole recursion so the total work done is capped regardless of how the cycle is
/// shaped (a NextIFD self-loop, a SubIFD pointing back at IFD0, etc).
fn walk_ifds(
    buf: &[u8],
    bo: ByteOrder,
    start: u32,
    visited: &mut HashSet<usize>,
    visited_count: &mut usize,
    candidates: &mut Vec<Candidate>,
) -> Result<(), String> {
    let mut offset = start as usize;
    while offset != 0 {
        if !visited.insert(offset) {
            break; // already visited this offset: a cycle in the NextIFD chain, stop quietly.
        }
        *visited_count += 1;
        if *visited_count > MAX_IFDS_VISITED {
            return Err("too many IFDs visited (possible cycle)".to_string());
        }

        let ifd = read_ifd(buf, bo, offset)?;
        collect_candidates(buf, bo, &ifd.entries, candidates);

        for entry in &ifd.entries {
            if entry.tag == TAG_SUB_IFDS || entry.tag == TAG_EXIF_IFD {
                let n = entry.count.min(MAX_POINTERS_PER_ENTRY) as usize;
                for i in 0..n {
                    if let Some(sub_offset) = entry.value_u32(buf, bo, i) {
                        if sub_offset != 0 {
                            walk_ifds(buf, bo, sub_offset, visited, visited_count, candidates)?;
                        }
                    }
                }
            }
        }

        offset = ifd.next as usize;
    }
    Ok(())
}

/// Parse `buf` as a TIFF-based raw file and return the bytes of the largest embedded JPEG found by the
/// IFD walk, if any. `Err` for anything that isn't a well-formed-enough TIFF container to walk at all
/// (too short, bad byte-order mark, bad magic, or the walk itself erroring out on a cycle/overflow);
/// `Ok(None)` for a structurally valid TIFF that simply has no embedded-JPEG candidate.
fn extract_largest_embedded_jpeg(buf: &[u8]) -> Result<Option<Vec<u8>>, String> {
    if buf.len() < 8 {
        return Err("file too short to be a TIFF container".to_string());
    }
    let bo = match &buf[0..2] {
        b"II" => ByteOrder::Little,
        b"MM" => ByteOrder::Big,
        _ => return Err("not a TIFF-based raw file (missing II/MM byte-order mark)".to_string()),
    };
    if bo.u16(&buf[2..4]) != 42 {
        return Err("not a TIFF-based raw file (bad TIFF magic number)".to_string());
    }
    let ifd0_offset = bo.u32(&buf[4..8]);

    let mut visited = HashSet::new();
    let mut visited_count = 0usize;
    let mut candidates = Vec::new();
    walk_ifds(buf, bo, ifd0_offset, &mut visited, &mut visited_count, &mut candidates)?;

    Ok(candidates.into_iter().max_by_key(|c| c.len).map(|c| buf[c.offset..c.offset + c.len].to_vec()))
}

/// Fallback for a walk that found nothing: `kamadak-exif`'s standard thumbnail tag pair
/// (`JPEGInterchangeFormat`/`Length` under `In::THUMBNAIL`), read from its own parsed TIFF buffer.
/// `None` on anything not present, not parseable, or not actually JPEG-shaped — never panics.
fn kamadak_thumbnail(buf: &[u8]) -> Option<Vec<u8>> {
    let mut cursor = std::io::Cursor::new(buf);
    let exif = exif::Reader::new().read_from_container(&mut cursor).ok()?;

    let off = exif.get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL)?.value.get_uint(0)?;
    let len = exif.get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In::THUMBNAIL)?.value.get_uint(0)?;
    let end = (off as usize).checked_add(len as usize)?;
    let bytes = exif.buf().get(off as usize..end)?;
    bytes.starts_with(&JPEG_SOI).then(|| bytes.to_vec())
}

fn to_data_url(jpeg: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg);
    format!("data:image/jpeg;base64,{b64}")
}

/// Extract the embedded JPEG preview from a camera-RAW file (`.cr2`/`.nef`/`.arw` — all TIFF containers)
/// and return it, bytes as-is (no re-encode), as a `data:image/jpeg;base64,...` URL. This is NOT
/// demosaicing: it never touches the raw sensor data, only whichever ready-made JPEG preview/thumbnail
/// the camera already embedded. `Err` if the file can't be parsed as a TIFF container at all, or parses
/// fine but carries no embedded JPEG the walk or the `kamadak-exif` fallback can find.
pub fn read_raw_preview_data_url(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;

    if let Ok(Some(jpeg)) = extract_largest_embedded_jpeg(&bytes) {
        return Ok(to_data_url(&jpeg));
    }

    if let Some(jpeg) = kamadak_thumbnail(&bytes) {
        return Ok(to_data_url(&jpeg));
    }

    Err("no embedded JPEG preview found in this raw file".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-camraw-{tag}"))
    }

    const TYPE_LONG: u16 = 4;

    fn u16b(v: u16, big_endian: bool) -> [u8; 2] {
        if big_endian { v.to_be_bytes() } else { v.to_le_bytes() }
    }

    fn u32b(v: u32, big_endian: bool) -> [u8; 4] {
        if big_endian { v.to_be_bytes() } else { v.to_le_bytes() }
    }

    /// The 4-byte value/offset slot for an inline (count==1) entry value, per the TIFF rule that a value
    /// shorter than 4 bytes is left-justified (stored in the first `size` bytes, zero-padded after) —
    /// mirrors `Entry::value_u32`'s read side so SHORT-typed entries (e.g. `Compression`) round-trip
    /// correctly in both byte orders, not just when `size == 4`.
    fn value_slot(typ: u16, value: u32, big_endian: bool) -> [u8; 4] {
        let size = match typ {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            _ => 4,
        };
        let full = u32b(value, big_endian);
        let mut out = [0u8; 4];
        if big_endian {
            out[..size].copy_from_slice(&full[4 - size..4]);
        } else {
            out[..size].copy_from_slice(&full[..size]);
        }
        out
    }

    fn entry_bytes(tag: u16, typ: u16, count: u32, value: u32, big_endian: bool) -> Vec<u8> {
        let mut e = Vec::with_capacity(12);
        e.extend_from_slice(&u16b(tag, big_endian));
        e.extend_from_slice(&u16b(typ, big_endian));
        e.extend_from_slice(&u32b(count, big_endian));
        e.extend_from_slice(&value_slot(typ, value, big_endian));
        e
    }

    fn ifd_bytes(entries: &[Vec<u8>], next: u32, big_endian: bool) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&u16b(entries.len() as u16, big_endian));
        for e in entries {
            b.extend_from_slice(e);
        }
        b.extend_from_slice(&u32b(next, big_endian));
        b
    }

    fn header(big_endian: bool, ifd0_offset: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(if big_endian { b"MM" } else { b"II" });
        b.extend_from_slice(&u16b(42, big_endian));
        b.extend_from_slice(&u32b(ifd0_offset, big_endian));
        b
    }

    /// A minimal JPEG-SOI-shaped blob of exactly `len` bytes: `FF D8`, filler, `FF D9`. The walker only
    /// ever inspects the leading SOI marker, so this is sufficient without a real JPEG encoder; `len`
    /// lets tests build two differently-sized "JPEGs" to check the largest-wins rule.
    fn tiny_jpeg(len: usize) -> Vec<u8> {
        assert!(len >= 4);
        let mut v = vec![0xFF, 0xD8];
        v.extend(std::iter::repeat_n(0x00, len - 4));
        v.push(0xFF);
        v.push(0xD9);
        v
    }

    /// Build a TIFF with IFD0 -> one SubIFD (0x014A) holding the JPEGInterchangeFormat/Length pair that
    /// points at `jpeg`. Layout is computed by hand so offsets line up exactly:
    /// header(8) -> IFD0 (1 entry, 18 bytes) -> SubIFD (2 entries, 30 bytes) -> jpeg bytes.
    fn build_single_preview_tiff(big_endian: bool, jpeg: &[u8]) -> Vec<u8> {
        let sub_ifd_offset: u32 = 8 + 18;
        let jpeg_offset: u32 = sub_ifd_offset + 30;

        let ifd0 = ifd_bytes(
            &[entry_bytes(TAG_SUB_IFDS, TYPE_LONG, 1, sub_ifd_offset, big_endian)],
            0,
            big_endian,
        );
        let sub_ifd = ifd_bytes(
            &[
                entry_bytes(TAG_JPEG_OFFSET, TYPE_LONG, 1, jpeg_offset, big_endian),
                entry_bytes(TAG_JPEG_LENGTH, TYPE_LONG, 1, jpeg.len() as u32, big_endian),
            ],
            0,
            big_endian,
        );

        let mut buf = header(big_endian, 8);
        assert_eq!(buf.len(), 8);
        buf.extend_from_slice(&ifd0);
        assert_eq!(buf.len(), sub_ifd_offset as usize);
        buf.extend_from_slice(&sub_ifd);
        assert_eq!(buf.len(), jpeg_offset as usize);
        buf.extend_from_slice(jpeg);
        buf
    }

    fn decode_data_url(url: &str) -> Vec<u8> {
        use base64::Engine;
        assert!(url.starts_with("data:image/jpeg;base64,"), "unexpected URL prefix: {url}");
        base64::engine::general_purpose::STANDARD
            .decode(url.trim_start_matches("data:image/jpeg;base64,"))
            .unwrap()
    }

    #[test]
    fn read_raw_preview_data_url_extracts_jpeg_little_endian() {
        let jpeg = tiny_jpeg(10);
        let buf = build_single_preview_tiff(false, &jpeg);

        let d = scratch("le");
        let f = d.join("a.cr2");
        fs::write(&f, &buf).unwrap();
        let url = read_raw_preview_data_url(&f.to_string_lossy()).unwrap();
        assert_eq!(decode_data_url(&url), jpeg);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_raw_preview_data_url_extracts_jpeg_big_endian() {
        let jpeg = tiny_jpeg(12);
        let buf = build_single_preview_tiff(true, &jpeg);

        let d = scratch("be");
        let f = d.join("a.nef");
        fs::write(&f, &buf).unwrap();
        let url = read_raw_preview_data_url(&f.to_string_lossy()).unwrap();
        assert_eq!(decode_data_url(&url), jpeg);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_raw_preview_data_url_picks_the_larger_of_two_embedded_jpegs() {
        let big_endian = false;
        let small_jpeg = tiny_jpeg(8);
        let big_jpeg = tiny_jpeg(30);

        // IFD0 carries its own (small) JPEGInterchangeFormat pair *and* a SubIFDs pointer; the SubIFD
        // carries the (larger) pair. header(8) -> IFD0 (3 entries, 42 bytes) -> SubIFD (2 entries, 30
        // bytes) -> small jpeg -> big jpeg.
        let sub_ifd_offset: u32 = 8 + 42;
        let small_jpeg_offset: u32 = sub_ifd_offset + 30;
        let big_jpeg_offset: u32 = small_jpeg_offset + small_jpeg.len() as u32;

        let ifd0 = ifd_bytes(
            &[
                entry_bytes(TAG_JPEG_OFFSET, TYPE_LONG, 1, small_jpeg_offset, big_endian),
                entry_bytes(TAG_JPEG_LENGTH, TYPE_LONG, 1, small_jpeg.len() as u32, big_endian),
                entry_bytes(TAG_SUB_IFDS, TYPE_LONG, 1, sub_ifd_offset, big_endian),
            ],
            0,
            big_endian,
        );
        let sub_ifd = ifd_bytes(
            &[
                entry_bytes(TAG_JPEG_OFFSET, TYPE_LONG, 1, big_jpeg_offset, big_endian),
                entry_bytes(TAG_JPEG_LENGTH, TYPE_LONG, 1, big_jpeg.len() as u32, big_endian),
            ],
            0,
            big_endian,
        );

        let mut buf = header(big_endian, 8);
        buf.extend_from_slice(&ifd0);
        assert_eq!(buf.len(), sub_ifd_offset as usize);
        buf.extend_from_slice(&sub_ifd);
        assert_eq!(buf.len(), small_jpeg_offset as usize);
        buf.extend_from_slice(&small_jpeg);
        assert_eq!(buf.len(), big_jpeg_offset as usize);
        buf.extend_from_slice(&big_jpeg);

        let d = scratch("two");
        let f = d.join("a.arw");
        fs::write(&f, &buf).unwrap();
        let url = read_raw_preview_data_url(&f.to_string_lossy()).unwrap();
        let decoded = decode_data_url(&url);
        assert_eq!(decoded, big_jpeg, "must pick the larger of the two embedded JPEGs");
        assert_ne!(decoded, small_jpeg);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_raw_preview_data_url_errs_when_there_is_no_embedded_preview() {
        // A well-formed TIFF whose IFD0 carries an unrelated tag only — no JPEGInterchangeFormat, no
        // SubIFDs/ExifIFD, no strip-based JPEG, and (being synthetic) no EXIF thumbnail for the
        // kamadak-exif fallback to find either.
        let big_endian = false;
        let ifd0 = ifd_bytes(&[entry_bytes(0x0100 /* ImageWidth */, TYPE_LONG, 1, 640, big_endian)], 0, big_endian);
        let mut buf = header(big_endian, 8);
        buf.extend_from_slice(&ifd0);

        let d = scratch("nopreview");
        let f = d.join("a.cr2");
        fs::write(&f, &buf).unwrap();
        assert!(read_raw_preview_data_url(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_raw_preview_data_url_rejects_malformed_input_without_panicking() {
        let d = scratch("bad");

        // Non-TIFF: doesn't start with II/MM.
        let f1 = d.join("not_tiff.cr2");
        fs::write(&f1, b"this is not a tiff file at all, just plain bytes").unwrap();

        // Truncated: valid II/MM + magic but no room for an IFD0 offset, let alone an IFD0.
        let f2 = d.join("truncated.cr2");
        fs::write(&f2, b"II*\x00").unwrap();

        // Empty file.
        let f3 = d.join("empty.cr2");
        fs::write(&f3, b"").unwrap();

        // IFD0 offset points past the end of the buffer.
        let big_endian = false;
        let f4 = d.join("bad_offset.cr2");
        fs::write(&f4, header(big_endian, 100_000)).unwrap();

        // Cyclic NextIFD chain: IFD0's next-IFD offset points right back at IFD0 itself.
        let f5 = d.join("cyclic_next.cr2");
        let cyclic_ifd0 = ifd_bytes(&[entry_bytes(0x0100, TYPE_LONG, 1, 1, big_endian)], 8, big_endian);
        let mut cyclic_buf = header(big_endian, 8);
        cyclic_buf.extend_from_slice(&cyclic_ifd0);
        fs::write(&f5, &cyclic_buf).unwrap();

        // Cyclic SubIFD: IFD0's SubIFDs pointer points back at IFD0 itself.
        let f6 = d.join("cyclic_sub.cr2");
        let cyclic_sub_ifd0 = ifd_bytes(&[entry_bytes(TAG_SUB_IFDS, TYPE_LONG, 1, 8, big_endian)], 0, big_endian);
        let mut cyclic_sub_buf = header(big_endian, 8);
        cyclic_sub_buf.extend_from_slice(&cyclic_sub_ifd0);
        fs::write(&f6, &cyclic_sub_buf).unwrap();

        for f in [&f1, &f2, &f3, &f4, &f5, &f6] {
            let path = f.to_string_lossy().to_string();
            let result = std::panic::catch_unwind(move || read_raw_preview_data_url(&path));
            let result = result.unwrap_or_else(|_| panic!("read_raw_preview_data_url panicked on {f:?}"));
            assert!(result.is_err(), "expected Err for malformed input {f:?}, got Ok");
        }

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_raw_preview_data_url_extracts_strip_based_jpeg_with_compression_6() {
        // Compression == 6 (old-style JPEG) with StripOffsets/StripByteCounts pointing at JPEG-shaped
        // bytes, instead of the JPEGInterchangeFormat pair.
        let big_endian = false;
        let jpeg = tiny_jpeg(16);
        let jpeg_offset: u32 = 8 + 2 + 12 * 3 + 4; // header + IFD0(3 entries)

        let ifd0 = ifd_bytes(
            &[
                entry_bytes(TAG_COMPRESSION, 3 /* SHORT */, 1, 6, big_endian),
                entry_bytes(TAG_STRIP_OFFSETS, TYPE_LONG, 1, jpeg_offset, big_endian),
                entry_bytes(TAG_STRIP_BYTE_COUNTS, TYPE_LONG, 1, jpeg.len() as u32, big_endian),
            ],
            0,
            big_endian,
        );
        let mut buf = header(big_endian, 8);
        buf.extend_from_slice(&ifd0);
        assert_eq!(buf.len(), jpeg_offset as usize);
        buf.extend_from_slice(&jpeg);

        let d = scratch("strip");
        let f = d.join("a.arw");
        fs::write(&f, &buf).unwrap();
        let url = read_raw_preview_data_url(&f.to_string_lossy()).unwrap();
        assert_eq!(decode_data_url(&url), jpeg);
        let _ = fs::remove_dir_all(&d);
    }
}
