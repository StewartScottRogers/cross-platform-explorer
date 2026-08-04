//! Video-metadata column extractor (CPE-1028, epic CPE-707): the video-family counterpart of
//! [`crate::image_column`] (image) and [`crate::media_column`] (audio).
//!
//! MP4/MOV files are **ISO-BMFF**: a flat sequence of *boxes*, each a big-endian `u32` size + 4-byte
//! ASCII type, optionally nested. The file's duration lives in the `moov` container box's `mvhd` child.
//! This module walks just enough of the box tree to reach `mvhd` and read `timescale`/`duration`, without
//! decoding any media — pure over bytes, no filesystem, adapter supplies the bytes. The box-header/child
//! primitives live in the shared [`crate::iso_bmff`] module (CPE-1309).

use crate::iso_bmff::find_child_box;
use crate::metadata_column::CellValue;

/// The [`CellValue::Float`] duration in seconds for a video's `bytes` (read from the ISO-BMFF
/// `moov/mvhd` box), or [`CellValue::Empty`] when the bytes aren't a parseable MP4/MOV — truncated,
/// malformed, or not ISO-BMFF at all. Never panics: every slice is bounds-checked first.
pub fn video_cell(bytes: &[u8]) -> CellValue {
    match duration_seconds(bytes) {
        Some(secs) => CellValue::Float(secs),
        None => CellValue::Empty,
    }
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    if end > bytes.len() {
        return None;
    }
    Some(u32::from_be_bytes(bytes[offset..end].try_into().ok()?))
}

fn read_u64_be(bytes: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    if end > bytes.len() {
        return None;
    }
    Some(u64::from_be_bytes(bytes[offset..end].try_into().ok()?))
}

/// Walk to `moov/mvhd` and compute `duration / timescale` in seconds, or `None` on any parse failure —
/// non-BMFF bytes, a missing `moov`/`mvhd`, truncation, or `timescale == 0`.
fn duration_seconds(bytes: &[u8]) -> Option<f64> {
    let (moov_start, moov_end) = find_child_box(bytes, 0, bytes.len(), b"moov")?;
    let (mvhd_start, mvhd_end) = find_child_box(bytes, moov_start, moov_end, b"mvhd")?;

    if mvhd_start >= mvhd_end || mvhd_start >= bytes.len() {
        return None;
    }
    let version = bytes[mvhd_start];

    let (timescale, duration) = if version == 0 {
        // version+flags(4) + creation(4) + modification(4) + timescale(4) + duration(4)
        let timescale = read_u32_be(bytes, mvhd_start + 12)? as u64;
        let duration = read_u32_be(bytes, mvhd_start + 16)? as u64;
        if mvhd_start + 20 > mvhd_end {
            return None;
        }
        (timescale, duration)
    } else if version == 1 {
        // version+flags(4) + creation(8) + modification(8) + timescale(4) + duration(8)
        let timescale = read_u32_be(bytes, mvhd_start + 20)? as u64;
        let duration = read_u64_be(bytes, mvhd_start + 24)?;
        if mvhd_start + 32 > mvhd_end {
            return None;
        }
        (timescale, duration)
    } else {
        return None; // unrecognised mvhd version
    };

    if timescale == 0 {
        return None;
    }
    Some(duration as f64 / timescale as f64)
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

    /// A version-0 `mvhd` body: version+flags, creation, modification, timescale, duration, then a
    /// trailing tail (rate/volume/matrix/… — irrelevant to the parser, just padding to look realistic).
    fn mvhd_v0(timescale: u32, duration: u32) -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(&[0, 0, 0, 0]); // version 0 + flags
        c.extend_from_slice(&0u32.to_be_bytes()); // creation_time
        c.extend_from_slice(&0u32.to_be_bytes()); // modification_time
        c.extend_from_slice(&timescale.to_be_bytes());
        c.extend_from_slice(&duration.to_be_bytes());
        c.extend_from_slice(&[0u8; 8]); // trailing padding
        c
    }

    /// A version-1 `mvhd` body: 64-bit creation/modification/duration, 32-bit timescale.
    fn mvhd_v1(timescale: u32, duration: u64) -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(&[1, 0, 0, 0]); // version 1 + flags
        c.extend_from_slice(&0u64.to_be_bytes()); // creation_time
        c.extend_from_slice(&0u64.to_be_bytes()); // modification_time
        c.extend_from_slice(&timescale.to_be_bytes());
        c.extend_from_slice(&duration.to_be_bytes());
        c.extend_from_slice(&[0u8; 8]); // trailing padding
        c
    }

    /// A minimal file: an `ftyp` box (as real MP4/MOV files start with) followed by a `moov` box
    /// containing the given `mvhd` bytes.
    fn synthetic_file(mvhd_content: &[u8]) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&make_box(b"ftyp", b"isom\0\0\x02\0isomiso2mp41"));
        let mvhd = make_box(b"mvhd", mvhd_content);
        f.extend_from_slice(&make_box(b"moov", &mvhd));
        f
    }

    #[test]
    fn valid_mvhd_v0_yields_known_seconds() {
        // 90000 units / 45000 timescale = 2.0 seconds.
        let file = synthetic_file(&mvhd_v0(45_000, 90_000));
        assert_eq!(video_cell(&file), CellValue::Float(2.0));
    }

    #[test]
    fn valid_mvhd_v1_64bit_yields_known_seconds() {
        // 1000 timescale, 12_345 duration units → 12.345 seconds.
        let file = synthetic_file(&mvhd_v1(1000, 12_345));
        assert_eq!(video_cell(&file), CellValue::Float(12.345));
    }

    #[test]
    fn zero_timescale_yields_empty() {
        let file = synthetic_file(&mvhd_v0(0, 90_000));
        assert_eq!(video_cell(&file), CellValue::Empty);
    }

    #[test]
    fn truncated_bytes_yield_empty_not_panic() {
        let file = synthetic_file(&mvhd_v0(45_000, 90_000));
        // Chop the file off at every prefix length — none of these may panic, and full truncations must
        // be Empty.
        for cut in [0, 1, 4, 8, 12, 20, file.len() / 2, file.len() - 1] {
            assert_eq!(video_cell(&file[..cut]), CellValue::Empty, "cut at {cut}");
        }
    }

    #[test]
    fn non_bmff_bytes_yield_empty() {
        assert_eq!(video_cell(b""), CellValue::Empty);
        assert_eq!(video_cell(b"not a video file, just text"), CellValue::Empty);
    }

    #[test]
    fn missing_moov_or_mvhd_yields_empty() {
        // Only an ftyp box, no moov at all.
        let mut f = Vec::new();
        f.extend_from_slice(&make_box(b"ftyp", b"isom"));
        assert_eq!(video_cell(&f), CellValue::Empty);

        // A moov box with no mvhd child.
        let mut f2 = Vec::new();
        let empty_moov = make_box(b"moov", &make_box(b"trak", b"whatever"));
        f2.extend_from_slice(&empty_moov);
        assert_eq!(video_cell(&f2), CellValue::Empty);
    }

    #[test]
    fn unrecognised_mvhd_version_yields_empty() {
        let mut bad = mvhd_v0(45_000, 90_000);
        bad[0] = 2; // neither version 0 nor 1
        let file = synthetic_file(&bad);
        assert_eq!(video_cell(&file), CellValue::Empty);
    }

    #[test]
    fn declared_size_larger_than_available_bytes_yields_empty() {
        // A moov box that claims a size far beyond what's actually present.
        let mut f = Vec::new();
        f.extend_from_slice(&1_000_000u32.to_be_bytes());
        f.extend_from_slice(b"moov");
        f.extend_from_slice(&mvhd_v0(1000, 1000));
        assert_eq!(video_cell(&f), CellValue::Empty);
    }
}
