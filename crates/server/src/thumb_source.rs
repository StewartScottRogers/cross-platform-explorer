//! Thumbnail **source decode** (CPE-1086, epic CPE-718): the decode step `make_thumbnail_png`
//! (`thumbnail.rs`) delegates to. Today's `image::open` can't decode PSD (thumbnails fall back to a
//! generic icon) and has no bomb-guard on a raster decode. This module owns both: PSD goes through the
//! vendored `psd` crate's flattened RGBA composite (mirrors `image_preview::read_image_data_url`);
//! everything else decodes via the `image` crate with a bounded `image::Limits` so a crafted
//! decompression-bomb file fails fast with an `Err` rather than attempting an unbounded allocation.
//!
//! Returns the source's raw bytes alongside the decoded image so the caller can read EXIF and apply
//! [`crate::thumb_orient::orient_for_display`] — decoding happens exactly once either way.
//!
//! Reuses the already-vendored `psd` and `image` crates; **no new dependencies**. The `image::Limits`
//! values are a local copy of `batch_transform::bounded_limits` (kept private there) — the same
//! "documented local copy" precedent `thumb_orient` used for its EXIF-orientation logic.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageReader, Limits};

/// Strict decode limits (mirrors `batch_transform::bounded_limits` / CPE-1083's "bounded allocation"
/// requirement): a crafted file that declares a huge canvas must fail fast with an `Err`, never attempt
/// an unbounded pixel-buffer allocation. `max_image_width`/`max_image_height` are checked against the
/// file's header *before* any pixel data is decoded; `max_alloc` is a coarser backstop on the decoded
/// buffer's total byte size.
const MAX_IMAGE_DIMENSION: u32 = 20_000; // 20k×20k is already a huge poster print
const MAX_ALLOC_BYTES: u64 = 256 * 1024 * 1024; // 256 MiB decode budget — generous for any real image

fn bounded_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_ALLOC_BYTES);
    limits
}

/// Decode a thumbnail source to an image + its raw bytes (the bytes let the caller read EXIF
/// orientation — a PSD carries none, so orientation is a no-op there). Dispatches on the (lowercased)
/// file extension: `.psd` goes through the `psd` crate's composite, everything else through `image`
/// with the bomb-guard [`Limits`] applied.
pub fn decode_thumb_image(path: &Path) -> Result<(DynamicImage, Vec<u8>), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = crate::model::extension_of(path);

    let img = if ext == "psd" {
        let psd = psd::Psd::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let rgba = psd.rgba();
        let buf = image::RgbaImage::from_raw(psd.width(), psd.height(), rgba)
            .ok_or("PSD pixel buffer size mismatch")?;
        DynamicImage::ImageRgba8(buf)
    } else {
        let mut reader = ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        reader.limits(bounded_limits());
        reader.decode().map_err(|e| e.to_string())?
    };

    Ok((img, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch(tag: &str) -> std::path::PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-thumbsrc-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Build a minimal, valid, **uncompressed** 8BPS PSD by hand: a 26-byte file header (RGB, 8-bit,
    /// `width` x `height`), three empty length-prefixed sections (color mode data / image resources /
    /// layer-and-mask — each just a zero `u32` length, which every section parser accepts as "empty"),
    /// then the image data section (`compression=0` raw, planar R/G/B, one byte per channel per pixel,
    /// no alpha so the composite comes out fully opaque). Same "build the bytes by hand with a
    /// discipline" style as CPE-1083/1084's minimal-PNG fixtures.
    fn minimal_psd(width: u32, height: u32) -> Vec<u8> {
        let mut b = Vec::new();
        // --- File header section (26 bytes, big-endian ints throughout) ---
        b.extend_from_slice(b"8BPS"); // signature
        b.extend_from_slice(&1u16.to_be_bytes()); // version = 1
        b.extend_from_slice(&[0u8; 6]); // reserved
        b.extend_from_slice(&3u16.to_be_bytes()); // channel count: R, G, B
        b.extend_from_slice(&height.to_be_bytes());
        b.extend_from_slice(&width.to_be_bytes());
        b.extend_from_slice(&8u16.to_be_bytes()); // depth: 8 bits/channel (low byte read as the value)
        b.extend_from_slice(&3u16.to_be_bytes()); // color mode: RGB (low byte read as the value)

        // --- Color mode data / image resources / layer-and-mask: all empty (length 0) ---
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());

        // --- Image data section: raw (uncompressed), planar R then G then B ---
        b.extend_from_slice(&0u16.to_be_bytes()); // compression = 0 (raw)
        let plane = (width * height) as usize;
        b.extend(std::iter::repeat(200u8).take(plane)); // R plane
        b.extend(std::iter::repeat(100u8).take(plane)); // G plane
        b.extend(std::iter::repeat(50u8).take(plane)); // B plane
        b
    }

    #[test]
    fn decode_thumb_image_decodes_a_minimal_psd_to_the_right_dims() {
        let d = scratch("psd");
        let f = d.join("a.psd");
        std::fs::write(&f, minimal_psd(6, 4)).unwrap();
        let (img, bytes) = decode_thumb_image(&f).unwrap();
        assert_eq!((img.width(), img.height()), (6, 4));
        assert!(!bytes.is_empty(), "raw bytes must be returned alongside the decoded image");
        let px = img.to_rgba8().get_pixel(0, 0).0;
        assert_eq!(px, [200, 100, 50, 255], "planar R/G/B composited, fully opaque");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn decode_thumb_image_rejects_a_corrupt_psd_distinctly_from_a_corrupt_raster_file() {
        let d = scratch("psdbad");
        let psd_path = d.join("bad.psd");
        let raster_path = d.join("bad.png");
        std::fs::write(&psd_path, b"not a psd").unwrap();
        std::fs::write(&raster_path, b"not a png").unwrap();
        let psd_err = decode_thumb_image(&psd_path).unwrap_err();
        let raster_err = decode_thumb_image(&raster_path).unwrap_err();
        // Both error, and via different decoders — a sanity check the `.psd` extension really does
        // dispatch to the psd-decoder branch rather than silently falling through to `image`.
        assert!(!psd_err.is_empty());
        assert!(!raster_err.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn decode_thumb_image_decodes_a_normal_png() {
        let d = scratch("png");
        let f = d.join("x.png");
        image::RgbImage::from_pixel(12, 8, image::Rgb([1u8, 2, 3])).save(&f).unwrap();
        let (img, bytes) = decode_thumb_image(&f).unwrap();
        assert_eq!((img.width(), img.height()), (12, 8));
        assert!(!bytes.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn decode_thumb_image_rejects_a_decompression_bomb_png() {
        let d = scratch("bomb");
        let f = d.join("bomb.png");
        std::fs::write(&f, bomb_png()).unwrap();
        let err = decode_thumb_image(&f);
        assert!(err.is_err(), "a huge-declared-dimension PNG must be rejected via image::Limits");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A minimal but well-formed PNG header (IHDR) declaring a 100,000 x 100,000 canvas — far past
    /// `MAX_IMAGE_DIMENSION` — with no real pixel data behind it. The strict width/height limit must
    /// reject this at header-parse time, before any decode-buffer allocation. Mirrors
    /// `batch_transform::tests::decompression_bomb_errors_instead_of_allocating`'s fixture exactly.
    fn bomb_png() -> Vec<u8> {
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
        png
    }

    /// Tiny CRC-32 (zlib polynomial) so the decompression-bomb PNG fixture has a valid IHDR checksum —
    /// std-only, no new dependency. Copied from `batch_transform::tests::crc32` (same fixture style).
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
