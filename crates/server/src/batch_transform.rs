//! Batch media **transform engine** (CPE-1083, epic CPE-723): the pure bytes→bytes counterpart to
//! [`batch_media`]'s planner. `batch_media::plan` only computes *where* each output goes; this module
//! actually applies the ordered [`batch_media::MediaOp`]s to an image's decoded pixels and re-encodes.
//!
//! Decode once into a [`image::DynamicImage`], fold every op in order (a `Convert` only changes the
//! *target* format — decoding always happens once, up front), then encode at the end. Every failure
//! (corrupt bytes, unsupported format, bad op) is a graceful `Err`, never a panic — this runs over
//! arbitrary user files.
//!
//! Reuses the already-vendored `image` (decoders/encoders) and `kamadak-exif` (EXIF orientation) crates;
//! **no new dependencies**.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, ImageReader, Limits};

use crate::batch_media::MediaOp;

/// Strict decode limits (CPE-1083 "bounded allocation" requirement): a crafted file that declares a
/// huge canvas (a "decompression bomb") must fail fast with an `Err`, never attempt to allocate an
/// unbounded pixel buffer. `max_image_width`/`max_image_height` are checked against the file's header
/// *before* any pixel data is decoded (the `image` crate's PNG/TIFF/etc. decoders enforce this
/// strictly); `max_alloc` is a second, coarser backstop on the decoded buffer's total byte size.
const MAX_IMAGE_DIMENSION: u32 = 20_000; // 20k×20k is already a huge poster print; anything past this is not a real photo
const MAX_ALLOC_BYTES: u64 = 256 * 1024 * 1024; // 256 MiB decode budget — generous for any real image

fn bounded_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_ALLOC_BYTES);
    limits
}

/// Map a (lowercased, dot-stripped) target extension to one of the **6 enabled** `image` encoders.
/// Anything else (heic/avif/psd/svg/…) is graceful — `None`, not a panic — the caller turns that into
/// an `Err`.
fn ext_to_format(ext: &str) -> Option<ImageFormat> {
    match ext.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "gif" => Some(ImageFormat::Gif),
        "webp" => Some(ImageFormat::WebP),
        "bmp" => Some(ImageFormat::Bmp),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        _ => None,
    }
}

/// Read the EXIF `Orientation` tag (1-8) from an image's leading bytes, if present. `None` on any
/// error (not an image, no EXIF, corrupt block) — never panics.
fn read_exif_orientation(bytes: &[u8]) -> Option<u32> {
    let exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()?;
    let field = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?;
    field.value.get_uint(0)
}

/// Bake the EXIF orientation hint into the pixels themselves via the matching rotate/flip, per the
/// standard EXIF orientation table. `1` (or any unrecognised value) is a no-op. Used by
/// [`MediaOp::StripMetadata`] so dropping the orientation tag doesn't silently re-orient a phone photo.
fn normalize_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
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

/// Decode `input` once, fold the ordered `ops` over the resulting [`DynamicImage`], then encode to the
/// resulting target format. `Err` (never a panic) on any decode/encode/unsupported-op failure.
///
/// - **Resize { max_px }** — downscale-only: `thumbnail()` itself does *not* refuse to upscale (it just
///   fits within the requested box), so this only calls it when the image actually exceeds `max_px` on
///   either edge; a smaller source is left untouched.
/// - **Convert { to_ext }** — changes the *target* encoder; decoding still only happens once, up front.
/// - **Rotate { degrees }** — 90/180/270 only (the planner already validates this; an unexpected value
///   here is still a graceful `Err`, not a panic).
/// - **Flip { horizontal }** — mirrors horizontally or vertically.
/// - **StripMetadata** — re-encodes from the decoded pixels, which carry no EXIF/IPTC/XMP, so the output
///   simply never has any to begin with. Honest caveat: a JPEG round-trips through recompression (a
///   lossless APP1-segment stripper that edits bytes in place without re-encoding is a documented
///   FOLLOW-UP, not built here). Because the EXIF orientation tag is what a lot of phone photos rely on
///   for correct display, this first normalizes orientation into the pixels ([`normalize_orientation`])
///   so stripping the tag doesn't silently re-orient the image for viewers that no longer see it.
/// - **Rename { .. }** — a filename-only concern the planner (`batch_media::plan`) already handles; a
///   no-op here since this function only ever sees bytes, never a path.
pub fn apply_ops(input: &[u8], ops: &[MediaOp]) -> Result<Vec<u8>, String> {
    let mut reader = ImageReader::new(Cursor::new(input))
        .with_guessed_format()
        .map_err(|e| format!("could not read image bytes: {e}"))?;
    reader.limits(bounded_limits());

    let source_format = reader
        .format()
        .ok_or_else(|| "unrecognized image format".to_string())?;
    let mut img = reader.decode().map_err(|e| e.to_string())?;
    let mut target_format = source_format;

    for op in ops {
        match op {
            MediaOp::Resize { max_px } => {
                let max_px = (*max_px).max(1);
                if img.width() > max_px || img.height() > max_px {
                    img = img.thumbnail(max_px, max_px);
                }
            }
            MediaOp::Convert { to_ext } => {
                target_format = ext_to_format(to_ext)
                    .ok_or_else(|| format!("unsupported convert target extension: {to_ext}"))?;
            }
            MediaOp::Rotate { degrees } => {
                img = match degrees {
                    90 => img.rotate90(),
                    180 => img.rotate180(),
                    270 => img.rotate270(),
                    other => return Err(format!("unsupported rotate angle: {other} (must be 90, 180 or 270)")),
                };
            }
            MediaOp::Flip { horizontal } => {
                img = if *horizontal { img.fliph() } else { img.flipv() };
            }
            MediaOp::StripMetadata => {
                if let Some(orientation) = read_exif_orientation(input) {
                    img = normalize_orientation(img, orientation);
                }
                // No further action needed: `img` is decoded pixels only, so encoding it below never
                // carries EXIF/IPTC/XMP along — the re-encode itself is the "strip".
            }
            MediaOp::Rename { .. } => {} // filename concern; the planner handles it, not this fn
        }
    }

    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, target_format).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbImage::from_pixel(w, h, image::Rgb([10u8, 20, 30]))
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// A PNG with an asymmetric pixel pattern (one corner marked) so a flip's effect is observable —
    /// mirrors thumbnail.rs's style of building tiny in-memory fixtures rather than reading real files.
    fn asymmetric_png(w: u32, h: u32) -> Vec<u8> {
        let mut img = image::RgbImage::from_pixel(w, h, image::Rgb([0u8, 0, 0]));
        img.put_pixel(0, 0, image::Rgb([255, 0, 0])); // top-left marker, only
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn resize_downscales_and_keeps_aspect() {
        let input = png_bytes(100, 40);
        let out = apply_ops(&input, &[MediaOp::Resize { max_px: 32 }]).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width(), 32);
        assert!(decoded.height() <= 16 && decoded.height() >= 10, "got {}", decoded.height());
    }

    #[test]
    fn resize_never_upscales_a_smaller_source() {
        let input = png_bytes(20, 20);
        let out = apply_ops(&input, &[MediaOp::Resize { max_px: 64 }]).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.width(), 20);
        assert_eq!(decoded.height(), 20);
    }

    #[test]
    fn convert_png_to_jpeg_changes_the_encoded_format() {
        let input = png_bytes(16, 16);
        let out = apply_ops(&input, &[MediaOp::Convert { to_ext: "jpg".into() }]).unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), ImageFormat::Jpeg);
    }

    #[test]
    fn convert_to_an_unsupported_extension_errors() {
        let input = png_bytes(4, 4);
        for bad in ["heic", "avif", "psd", "svg"] {
            let err = apply_ops(&input, &[MediaOp::Convert { to_ext: bad.into() }]);
            assert!(err.is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn rotate90_swaps_dimensions() {
        let input = png_bytes(10, 4);
        let out = apply_ops(&input, &[MediaOp::Rotate { degrees: 90 }]).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 10));
    }

    #[test]
    fn rotate_rejects_a_non_right_angle() {
        let input = png_bytes(4, 4);
        assert!(apply_ops(&input, &[MediaOp::Rotate { degrees: 45 }]).is_err());
    }

    #[test]
    fn flip_horizontal_moves_the_asymmetric_pixel() {
        let input = asymmetric_png(4, 4);
        let out = apply_ops(&input, &[MediaOp::Flip { horizontal: true }]).unwrap();
        let decoded = image::load_from_memory(&out).unwrap().to_rgb8();
        // The top-left marker mirrors to the top-right on a horizontal flip.
        assert_eq!(decoded.get_pixel(3, 0).0, [255, 0, 0]);
        assert_eq!(decoded.get_pixel(0, 0).0, [0, 0, 0]);
    }

    #[test]
    fn flip_vertical_moves_the_asymmetric_pixel() {
        let input = asymmetric_png(4, 4);
        let out = apply_ops(&input, &[MediaOp::Flip { horizontal: false }]).unwrap();
        let decoded = image::load_from_memory(&out).unwrap().to_rgb8();
        // The top-left marker mirrors to the bottom-left on a vertical flip.
        assert_eq!(decoded.get_pixel(0, 3).0, [255, 0, 0]);
        assert_eq!(decoded.get_pixel(0, 0).0, [0, 0, 0]);
    }

    /// Build a tiny JPEG carrying an EXIF block (Orientation = 6, i.e. "rotate 90° CW to display
    /// correctly") using raw APP1/Exif segment bytes prepended to a minimal JPEG, mirroring how
    /// `media_meta_read.rs`'s EXIF tests construct fixtures without a real camera file. `kamadak-exif`
    /// auto-detects the container, so a `Reader::read_from_container` call on these bytes finds the tag.
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

    #[test]
    fn strip_metadata_drops_exif() {
        let input = jpeg_with_exif_orientation(8, 8, 1);
        assert!(read_exif_orientation(&input).is_some(), "fixture should carry EXIF");

        let out = apply_ops(&input, &[MediaOp::StripMetadata]).unwrap();
        assert!(read_exif_orientation(&out).is_none(), "output must not carry EXIF");
    }

    #[test]
    fn strip_metadata_normalizes_orientation_6_into_pixels() {
        // A 10x4 (wide) image tagged orientation=6 ("rotate 90 CW to display correctly") should come
        // out of StripMetadata as a physically-rotated 4x10 image, since the EXIF hint that would have
        // told a viewer to rotate it is about to be dropped.
        let input = jpeg_with_exif_orientation(10, 4, 6);
        let out = apply_ops(&input, &[MediaOp::StripMetadata]).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 10));
    }

    #[test]
    fn decompression_bomb_errors_instead_of_allocating() {
        // A minimal but well-formed PNG header (IHDR) declaring a huge canvas (100_000 x 100_000 — far
        // past MAX_IMAGE_DIMENSION), with no real pixel data behind it. The strict width/height limit
        // must reject this at header-parse time, before any decode-buffer allocation.
        let mut png = Vec::new();
        png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]); // signature
        let mut ihdr_data = Vec::new();
        ihdr_data.extend_from_slice(&100_000u32.to_be_bytes()); // width
        ihdr_data.extend_from_slice(&100_000u32.to_be_bytes()); // height
        ihdr_data.push(8); // bit depth
        ihdr_data.push(2); // color type: truecolor
        ihdr_data.push(0); // compression
        ihdr_data.push(0); // filter
        ihdr_data.push(0); // interlace
        png.extend_from_slice(&(ihdr_data.len() as u32).to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&ihdr_data);
        let crc = crc32(b"IHDR", &ihdr_data);
        png.extend_from_slice(&crc.to_be_bytes());
        // No IDAT / IEND needed — the decoder must fail on the header's declared dimensions alone.

        let err = apply_ops(&png, &[MediaOp::Resize { max_px: 32 }]);
        assert!(err.is_err(), "a huge-declared-dimension image must be rejected, not decoded");
    }

    /// Tiny CRC-32 (zlib polynomial) so the decompression-bomb PNG fixture has a valid IHDR checksum —
    /// std-only, no new dependency (the `flate2`/`zip` crates already vendored don't expose a bare CRC
    /// helper we can reuse here).
    fn crc32(chunk_type: &[u8], data: &[u8]) -> u32 {
        fn update(mut crc: u32, bytes: &[u8]) -> u32 {
            for &b in bytes {
                crc ^= b as u32;
                for _ in 0..8 {
                    crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
                }
            }
            crc
        }
        let crc = update(0xFFFF_FFFF, chunk_type);
        let crc = update(crc, data);
        !crc
    }
}
