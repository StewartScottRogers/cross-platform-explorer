//! Image-metadata column extractor (CPE-974, epic CPE-707): the image-family counterpart of
//! [`crate::media_column`] (audio).
//!
//! [`crate::metadata_column`] (CPE-918) already sorts a [`CellValue::Dimensions`] cell by pixel area; this
//! module produces that cell from a file's bytes. It reads only the image **header** (no full decode), so a
//! *Dimensions* column is cheap enough to fill per visible row. Pure over bytes — the adapter reads the
//! leading bytes and dispatches by kind; nothing here touches the filesystem.

use std::io::Cursor;

use crate::metadata_column::CellValue;

/// The [`CellValue::Dimensions`] for an image's `bytes`, or [`CellValue::Empty`] when the bytes aren't a
/// recognisable image (so the column sorts it last, per [`crate::metadata_column`]). Header-only: the format
/// is guessed and the dimensions read without decoding the pixels.
pub fn image_dimensions_cell(bytes: &[u8]) -> CellValue {
    match read_dimensions(bytes) {
        Some((w, h)) => CellValue::Dimensions { w, h },
        None => CellValue::Empty,
    }
}

/// Read `(width, height)` from an image header without decoding it, or `None` if the bytes aren't a
/// supported/parseable image.
fn read_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};

    /// Encode a solid `w×h` image in `format` to bytes — a real fixture, no external files.
    fn encode(w: u32, h: u32, format: ImageFormat) -> Vec<u8> {
        let img = RgbImage::new(w, h);
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), format).unwrap();
        buf
    }

    #[test]
    fn reads_png_dimensions_from_header() {
        let png = encode(640, 480, ImageFormat::Png);
        assert_eq!(image_dimensions_cell(&png), CellValue::Dimensions { w: 640, h: 480 });
    }

    #[test]
    fn reads_other_formats() {
        assert_eq!(image_dimensions_cell(&encode(32, 16, ImageFormat::Bmp)), CellValue::Dimensions { w: 32, h: 16 });
        assert_eq!(image_dimensions_cell(&encode(8, 8, ImageFormat::Gif)), CellValue::Dimensions { w: 8, h: 8 });
    }

    #[test]
    fn non_image_bytes_yield_empty() {
        assert_eq!(image_dimensions_cell(b""), CellValue::Empty);
        assert_eq!(image_dimensions_cell(b"not an image, just text"), CellValue::Empty);
        // A PNG magic prefix but garbage body → still Empty, not a panic.
        assert_eq!(image_dimensions_cell(&[0x89, b'P', b'N', b'G', 0, 1, 2, 3]), CellValue::Empty);
    }

    #[test]
    fn dimensions_sort_by_area_via_metadata_column() {
        use crate::metadata_column::compare;
        use std::cmp::Ordering;
        let small = image_dimensions_cell(&encode(10, 10, ImageFormat::Png)); // area 100
        let large = image_dimensions_cell(&encode(20, 20, ImageFormat::Png)); // area 400
        assert_eq!(compare(&small, &large, true), Ordering::Less);
    }
}
