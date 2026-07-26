//! Thumbnail **EXIF-orientation correction** (CPE-1085, epic CPE-718): phone photos carry an EXIF
//! `Orientation` tag rather than physically-rotated pixels, so a thumbnail generated straight from
//! the decoded bytes can come out sideways or upside-down. This module reads that tag and bakes it
//! into the pixels via the standard 8-value rotate/flip table, giving the thumbnail pipeline one
//! small, reusable `orient_for_display` call.
//!
//! The EXIF-reading + orientation-table logic already exists as **private** functions in
//! [`crate::batch_transform`] (`read_exif_orientation` / `normalize_orientation`), proven there by
//! CPE-1083's tests. Rather than make those `pub` (widening batch_transform's surface for an
//! unrelated caller) or introduce a shared-but-awkward re-export, this is a documented local copy —
//! the same pattern CPE-1079 used for its rename-marker copy. Do not edit `batch_transform.rs`.
//!
//! Reuses the already-vendored `image` (decode + rotate/flip) and `kamadak-exif` (EXIF parsing)
//! crates; **no new dependencies**.

use std::io::Cursor;

use image::DynamicImage;

/// Read the EXIF `Orientation` tag (1-8) from an image's leading bytes, if present. `None` on any
/// error (not an image, no EXIF, corrupt block) — never panics. Mirrors
/// `batch_transform::read_exif_orientation`.
pub fn read_exif_orientation(bytes: &[u8]) -> Option<u32> {
    let exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()?;
    let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
    field.value.get_uint(0)
}

/// Bake an EXIF orientation hint into `img`'s pixels via the matching rotate/flip, per the
/// standard EXIF orientation table. `1` (or any unrecognised value) is a no-op. Mirrors
/// `batch_transform::normalize_orientation` exactly.
pub fn apply_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),  // transpose
        6 => img.rotate90(),
        7 => img.rotate270().fliph(), // transverse
        8 => img.rotate270(),
        _ => img, // 1 = normal, or an orientation value we don't recognise
    }
}

/// Read `bytes`' EXIF orientation (if any) and apply it to `img` so the result is display-ready —
/// a no-op when the tag is absent or unrecognised. The one entry point the thumbnail pipeline
/// calls: decode once, then `orient_for_display(decoded, original_bytes)` before resizing/encoding.
pub fn orient_for_display(img: DynamicImage, bytes: &[u8]) -> DynamicImage {
    match read_exif_orientation(bytes) {
        Some(orientation) => apply_orientation(img, orientation),
        None => img,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageFormat;

    /// Build a tiny JPEG carrying an EXIF block with the given `Orientation` value, using raw
    /// APP1/Exif segment bytes prepended to a minimal JPEG — copied from
    /// `batch_transform::tests::jpeg_with_exif_orientation`, which itself mirrors
    /// `media_meta_read.rs`'s EXIF fixture style. `kamadak-exif` auto-detects the container, so a
    /// `Reader::read_from_container` call on these bytes finds the tag.
    fn jpeg_with_exif_orientation(w: u32, h: u32, orientation: u16) -> Vec<u8> {
        // Minimal valid TIFF-in-EXIF block: byte order (II, little-endian), magic 42, IFD0 offset 8,
        // one entry (Orientation, tag 0x0112, type SHORT=3, count 1, value in the first 2 bytes of the
        // 4-byte value field), then the next-IFD offset (0 = none).
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II"); // little-endian
        tiff.extend_from_slice(&42u16.to_le_bytes());
        tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at offset 8
        tiff.extend_from_slice(&1u16.to_le_bytes()); // 1 entry
        tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation tag
        tiff.extend_from_slice(&3u16.to_le_bytes()); // type SHORT
        tiff.extend_from_slice(&1u32.to_le_bytes()); // count 1
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0]); // pad the 4-byte value field
        tiff.extend_from_slice(&0u32.to_le_bytes()); // no next IFD

        let mut app1 = Vec::new();
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);

        let mut jpeg = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(w, h, image::Rgb([5u8, 6, 7]))
            .write_to(&mut jpeg, ImageFormat::Jpeg)
            .unwrap();
        let plain = jpeg.into_inner();

        // Splice the APP1/Exif segment in right after the SOI marker (FF D8).
        let mut out = Vec::new();
        out.extend_from_slice(&plain[..2]);
        out.push(0xFF);
        out.push(0xE1); // APP1
        let seg_len = (app1.len() + 2) as u16; // length field includes itself, excludes the marker
        out.extend_from_slice(&seg_len.to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&plain[2..]);
        out
    }

    fn plain_jpeg(w: u32, h: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(w, h, image::Rgb([1u8, 2, 3]))
            .write_to(&mut out, ImageFormat::Jpeg)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn orientation_6_rotates_a_wide_image_to_portrait() {
        // 10x4 (wide) tagged orientation=6 ("rotate 90 CW to display correctly") must come out of
        // orient_for_display as portrait (height > width).
        let input = jpeg_with_exif_orientation(10, 4, 6);
        let decoded = image::load_from_memory(&input).unwrap();
        let out = orient_for_display(decoded, &input);
        assert!(out.height() > out.width(), "expected portrait, got {}x{}", out.width(), out.height());
        assert_eq!((out.width(), out.height()), (4, 10));
    }

    #[test]
    fn orientation_1_is_unchanged() {
        let input = jpeg_with_exif_orientation(10, 4, 1);
        let decoded = image::load_from_memory(&input).unwrap();
        let (w, h) = (decoded.width(), decoded.height());
        let out = orient_for_display(decoded, &input);
        assert_eq!((out.width(), out.height()), (w, h));
    }

    #[test]
    fn no_exif_bytes_are_unchanged() {
        let input = plain_jpeg(10, 4);
        let decoded = image::load_from_memory(&input).unwrap();
        let (w, h) = (decoded.width(), decoded.height());
        assert!(read_exif_orientation(&input).is_none());
        let out = orient_for_display(decoded, &input);
        assert_eq!((out.width(), out.height()), (w, h));
    }

    #[test]
    fn every_orientation_value_and_a_bogus_one_never_panics() {
        for orientation in [1u32, 2, 3, 4, 5, 6, 7, 8, 99] {
            let input = jpeg_with_exif_orientation(10, 4, orientation as u16);
            let decoded = image::load_from_memory(&input).unwrap();
            let out = apply_orientation(decoded, orientation);
            // No assertion beyond "didn't panic" plus a sanity check the pixel count is conserved.
            assert_eq!(out.width() as u64 * out.height() as u64, 40);
        }
    }

    #[test]
    fn read_exif_orientation_reads_the_tagged_value() {
        let input = jpeg_with_exif_orientation(8, 8, 6);
        assert_eq!(read_exif_orientation(&input), Some(6));
    }
}
