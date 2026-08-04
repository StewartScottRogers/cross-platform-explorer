//! Shared **ISO-BMFF** (MP4/MOV) box-walking primitives (CPE-1309, epic CPE-725).
//!
//! MP4/MOV files are ISO-BMFF: a flat, potentially-nested sequence of *boxes*, each a big-endian `u32`
//! size + 4-byte ASCII type, with an optional 64-bit largesize when `size == 1`, or "runs to the end of
//! the enclosing range" when `size == 0`. Three consumers walk this structure and previously each carried
//! its own byte-identical copy of the header reader + child finder:
//! [`crate::video_column`] (duration from `moov/mvhd`), [`crate::video_meta_read`] (iTunes tags from
//! `moov/udta/meta/ilst`), and now [`crate::video_meta_write`] (metadata write-back). This module is the
//! single shared implementation all three use.
//!
//! Every size is clamped to the enclosing range, so a lying/huge declared size can't cause an over-read,
//! and the reader returns `None` (never panics) on any truncation or internally-inconsistent size. These
//! primitives are deliberately minimal and bounds-checked — the write side ([`crate::video_meta_write`])
//! layers its own byte-range copying + bottom-up size recomputation on top of them.

/// A parsed ISO-BMFF box header. The byte range `[header_start, box_end)` covers the whole box (header +
/// payload); `content_start` is where the payload begins. The 4-byte box *type* always lives at
/// `header_start + 4 .. header_start + 8`, regardless of 32-bit vs 64-bit-largesize encoding — the write
/// side relies on that to shadow a box in place (overwrite only its type).
pub(crate) struct BoxHeader {
    pub box_type: [u8; 4],
    pub header_start: usize,
    pub content_start: usize,
    pub box_end: usize,
}

/// Read the box header at `offset`, bounded by `end` (the enclosing box's content end, or `bytes.len()`
/// at the top level). `None` on any truncation or an internally inconsistent size — never panics. Every
/// size is clamped to `end` so a lying/huge declared size can't cause an over-read.
pub(crate) fn read_box_header(bytes: &[u8], offset: usize, end: usize) -> Option<BoxHeader> {
    if offset.checked_add(8)? > end || end > bytes.len() {
        return None;
    }
    let size32 = u32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?);
    let mut box_type = [0u8; 4];
    box_type.copy_from_slice(&bytes[offset + 4..offset + 8]);

    let (total_size, header_len): (u64, u64) = if size32 == 1 {
        // 64-bit largesize follows the type.
        if offset.checked_add(16)? > end {
            return None;
        }
        let largesize = u64::from_be_bytes(bytes[offset + 8..offset + 16].try_into().ok()?);
        (largesize, 16)
    } else if size32 == 0 {
        // Box runs to the end of the enclosing range.
        ((end - offset) as u64, 8)
    } else {
        (size32 as u64, 8)
    };

    if total_size < header_len {
        return None; // internally inconsistent — declared size smaller than its own header
    }
    let box_end = offset.checked_add(usize::try_from(total_size).ok()?)?;
    if box_end > end {
        return None; // truncated / overruns the enclosing box — clamp by rejecting rather than over-reading
    }
    let content_start = offset + usize::try_from(header_len).ok()?;
    Some(BoxHeader { box_type, header_start: offset, content_start, box_end })
}

/// The raw declared 32-bit size word of the box at `offset` (`0` = runs-to-end, `1` = 64-bit largesize
/// follows), or `None` if there aren't even 4 bytes there. The write side uses this to refuse a top-level
/// `size == 0` box (which would silently swallow appended bytes).
pub(crate) fn raw_box_size32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes(slice.try_into().ok()?))
}

/// Find the first child box of type `target` directly inside `[start, end)`, returning its *content* range
/// `(content_start, box_end)`. `None` if not found or the box tree is malformed/truncated at some point
/// during the walk.
pub(crate) fn find_child_box(bytes: &[u8], start: usize, end: usize, target: &[u8; 4]) -> Option<(usize, usize)> {
    let mut offset = start;
    while offset < end {
        let header = read_box_header(bytes, offset, end)?;
        if &header.box_type == target {
            return Some((header.content_start, header.box_end));
        }
        offset = header.box_end;
    }
    None
}

/// Serialize a box (`box_type` + `content`) onto the tail of `out`, choosing the 32-bit size encoding when
/// the total fits and the 64-bit largesize encoding otherwise — so an unexpectedly huge `moov` still
/// serializes correctly rather than truncating its size word. Metadata boxes are always small (32-bit);
/// the 64-bit path is defensive.
pub(crate) fn push_box(out: &mut Vec<u8>, box_type: &[u8; 4], content: &[u8]) {
    let content_len = content.len() as u64;
    if 8 + content_len <= u32::MAX as u64 {
        out.extend_from_slice(&((8 + content_len) as u32).to_be_bytes());
        out.extend_from_slice(box_type);
        out.extend_from_slice(content);
    } else {
        // 64-bit largesize: size32 == 1, then the type, then a u64 covering the full 16-byte header + body.
        out.extend_from_slice(&1u32.to_be_bytes());
        out.extend_from_slice(box_type);
        out.extend_from_slice(&(16 + content_len).to_be_bytes());
        out.extend_from_slice(content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_box(box_type: &[u8; 4], content: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&((8 + content.len()) as u32).to_be_bytes());
        b.extend_from_slice(box_type);
        b.extend_from_slice(content);
        b
    }

    #[test]
    fn reads_a_simple_header_and_finds_a_child() {
        let inner = make_box(b"mvhd", &[1, 2, 3, 4]);
        let moov = make_box(b"moov", &inner);
        let h = read_box_header(&moov, 0, moov.len()).unwrap();
        assert_eq!(&h.box_type, b"moov");
        assert_eq!(h.header_start, 0);
        assert_eq!(h.content_start, 8);
        assert_eq!(h.box_end, moov.len());
        let (cs, ce) = find_child_box(&moov, h.content_start, h.box_end, b"mvhd").unwrap();
        assert_eq!(&moov[cs..ce], &[1, 2, 3, 4]);
        assert!(find_child_box(&moov, h.content_start, h.box_end, b"trak").is_none());
    }

    #[test]
    fn raw_size_zero_and_one_are_reported() {
        let mut b = Vec::new();
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(b"mdat");
        assert_eq!(raw_box_size32(&b, 0), Some(0));
        assert_eq!(raw_box_size32(b"\0\0\0\x01moov", 0), Some(1));
        assert_eq!(raw_box_size32(b"abc", 0), None); // fewer than 4 bytes
    }

    #[test]
    fn truncated_or_inconsistent_headers_yield_none_not_panic() {
        assert!(read_box_header(b"", 0, 0).is_none());
        assert!(read_box_header(b"\0\0\0", 0, 3).is_none());
        // Declared size (4) smaller than the 8-byte header.
        assert!(read_box_header(b"\0\0\0\x04moov", 0, 8).is_none());
        // Declared size overruns the enclosing range.
        assert!(read_box_header(b"\0\0\x10\x00moov", 0, 8).is_none());
    }

    #[test]
    fn push_box_round_trips_through_read_box_header() {
        let mut out = Vec::new();
        push_box(&mut out, b"free", b"payload-bytes");
        let h = read_box_header(&out, 0, out.len()).unwrap();
        assert_eq!(&h.box_type, b"free");
        assert_eq!(&out[h.content_start..h.box_end], b"payload-bytes");
    }
}
