//! Image compare (CPE-1490, epic CPE-722): the image-compare scope of the CPE-722 compare studio that
//! CPE-779 (folder/binary/text compare) explicitly deferred. This module is the headless **engine**
//! only — a side-by-side / onion-skin / heatmap pane wired to it is the separate follow-up CPE-1508.
//!
//! [`diff_images`] decodes both inputs through the thumbnail pipeline's already-bomb-guarded decode
//! ([`crate::thumb_source::decode_thumb_image`], CPE-1447/CPE-1449 — 20,000×20,000 / 256 MiB caps,
//! source-file-size gate before the read) rather than adding a second image-decode path, then computes
//! a per-pixel delta: a grayscale diff-mask PNG (brighter = larger delta) plus summary stats
//! (`percent_different`, `changed_pixels`, and a bounding box of the changed region).
//!
//! **Differing dimensions** are handled by aligning both images at the top-left corner of their union
//! bounding box (`max(width_a, width_b)` x `max(height_a, height_b)`) rather than rejecting the pair or
//! scaling either one — scaling would distort a real size difference into a false pixel match. Any
//! coordinate that falls outside one image's own extent has no counterpart to compare against, so it is
//! scored as maximally different (delta 255); `ImageDiff::size_mismatch` flags the pair so callers don't
//! have to infer it from the bbox. This never panics on a size mismatch, and is exercised by a test.
//!
//! **Bounded, twice over:** `decode_thumb_image` already refuses an oversized *source* file/declared
//! canvas before it decodes. On top of that, [`MAX_DIFF_DIMENSION`] bounds the union canvas this module
//! will actually *diff* — a diff multiplies the cost of one decode by two images plus a same-sized mask
//! buffer, so even two images that individually clear the thumbnail pipeline's 20,000×20,000 decode cap
//! (each up to ~1.6 GB as RGBA) are refused here rather than materializing three such buffers. No new
//! dependency: reuses the in-tree `image` crate exactly as `thumbnail.rs` does.

use std::path::Path;

use image::{Rgba, RgbaImage};
use serde::Serialize;

use crate::thumb_source::decode_thumb_image;

/// A rectangle bounding the changed-pixel region, in the aligned diff-canvas coordinate space
/// (top-left origin, inclusive extent). Absent from [`ImageDiff`] when nothing differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DiffBBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Result of [`diff_images`]. `mask_png` is a grayscale-in-RGBA PNG, `width`x`height` (the union
/// bounding box of the two inputs — equal to both inputs' own size unless `size_mismatch`), where each
/// pixel's brightness is that coordinate's per-channel-averaged delta (0 = identical, 255 = maximally
/// different) — handed to the UI as a heatmap overlay by the separate CPE-1508 GUI pane.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ImageDiff {
    pub width: u32,
    pub height: u32,
    pub changed_pixels: u64,
    pub total_pixels: u64,
    pub percent_different: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox: Option<DiffBBox>,
    /// `true` when the two source images didn't share the same dimensions — see the module docs for
    /// how the pair is aligned in that case (union-bbox, top-left origin, out-of-bounds = max delta).
    pub size_mismatch: bool,
    pub mask_png: Vec<u8>,
}

/// Cap on the union canvas a diff will actually be computed over (see the module docs' "bounded, twice
/// over" note). Chosen well under `thumb_source`'s own 20,000-px decode cap: at 4096×4096, the diff's
/// two RGBA source buffers plus the RGBA mask buffer top out around 200 MiB combined — generous for any
/// real compare-pane use (photos, screenshots, icons) without inviting a multi-gigabyte spike from a
/// pair of poster-sized originals.
const MAX_DIFF_DIMENSION: u32 = 4096;

/// Decode edge handed to `decode_thumb_image` for the diff's own full-resolution read. Only affects the
/// SVG/font/PDF/video branches (which render directly at ~this size); the raster/PSD branches always
/// decode at their native resolution regardless of this value. Matches [`MAX_DIFF_DIMENSION`] so a
/// vector source renders at a size worth diffing without exceeding the cap outright.
const DECODE_EDGE: u32 = MAX_DIFF_DIMENSION;

/// Diff two image files pixel-by-pixel (CPE-1490).
///
/// Decodes both via [`decode_thumb_image`] — an unreadable path, unsupported/corrupt format, or a
/// source that fails that decoder's own bomb-guard is `Err`, never a panic. If the union of the two
/// decoded sizes exceeds [`MAX_DIFF_DIMENSION`] on either axis, the pair is refused with `Err` before
/// any diff-sized buffer is allocated (both source images are already fully decoded in memory at that
/// point via `decode_thumb_image`'s own — looser — bound, but no diff/mask buffer is built).
///
/// Same-size inputs compare 1:1. Differing sizes are aligned at the top-left corner of the union
/// bounding box; see the module docs for how the padding region is scored.
pub fn diff_images(a: &Path, b: &Path) -> Result<ImageDiff, String> {
    let (img_a, _) = decode_thumb_image(a, DECODE_EDGE)?;
    let (img_b, _) = decode_thumb_image(b, DECODE_EDGE)?;
    let (wa, ha) = (img_a.width(), img_a.height());
    let (wb, hb) = (img_b.width(), img_b.height());
    let width = wa.max(wb);
    let height = ha.max(hb);
    if width > MAX_DIFF_DIMENSION || height > MAX_DIFF_DIMENSION {
        return Err(format!(
            "image too large to diff ({width}x{height}; limit {MAX_DIFF_DIMENSION}x{MAX_DIFF_DIMENSION})"
        ));
    }
    let size_mismatch = wa != wb || ha != hb;
    let ra = img_a.to_rgba8();
    let rb = img_b.to_rgba8();

    let mut mask = RgbaImage::new(width, height);
    let mut changed: u64 = 0u64;
    let total = width as u64 * height as u64;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0u32, 0u32);

    for y in 0..height {
        for x in 0..width {
            let in_a = x < wa && y < ha;
            let in_b = x < wb && y < hb;
            let delta: u8 = if in_a && in_b {
                channel_delta(*ra.get_pixel(x, y), *rb.get_pixel(x, y))
            } else {
                // No counterpart on one side (a differing-dimension pair) — nothing to compare against,
                // scored as maximally different so the padding region reads as fully "changed" in the
                // heatmap rather than silently matching.
                255
            };
            if delta > 0 {
                changed += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            mask.put_pixel(x, y, Rgba([delta, delta, delta, 255]));
        }
    }

    let bbox = (changed > 0)
        .then(|| DiffBBox { x: min_x, y: min_y, width: max_x - min_x + 1, height: max_y - min_y + 1 });
    let percent_different = if total == 0 { 0.0 } else { (changed as f64 / total as f64) * 100.0 };

    let mut buf = std::io::Cursor::new(Vec::new());
    mask.write_to(&mut buf, image::ImageFormat::Png).map_err(|e| e.to_string())?;

    Ok(ImageDiff {
        width,
        height,
        changed_pixels: changed,
        total_pixels: total,
        percent_different,
        bbox,
        size_mismatch,
        mask_png: buf.into_inner(),
    })
}

/// Mean absolute per-channel delta between two RGBA pixels, 0-255 (the mask's grayscale intensity at
/// that coordinate). Averaging (rather than, say, max) keeps a small alpha-only or single-channel
/// change from painting as a full-white pixel while a genuine full-RGBA flip still reads near-white.
fn channel_delta(a: Rgba<u8>, b: Rgba<u8>) -> u8 {
    let sum: u32 = a.0.iter().zip(b.0.iter()).map(|(&x, &y)| (x as i32 - y as i32).unsigned_abs()).sum();
    (sum / 4) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-imgdiff-{tag}"))
    }

    fn save_png(dir: &Path, name: &str, w: u32, h: u32, rgb: [u8; 3]) -> std::path::PathBuf {
        let f = dir.join(name);
        image::RgbImage::from_pixel(w, h, image::Rgb(rgb)).save(&f).unwrap();
        f
    }

    #[test]
    fn identical_images_are_zero_percent_different_with_an_empty_mask() {
        let d = scratch("identical");
        let a = save_png(&d, "a.png", 10, 10, [10, 20, 30]);
        let b = save_png(&d, "b.png", 10, 10, [10, 20, 30]);
        let diff = diff_images(&a, &b).unwrap();
        assert_eq!(diff.changed_pixels, 0);
        assert_eq!(diff.percent_different, 0.0);
        assert_eq!(diff.total_pixels, 100);
        assert!(diff.bbox.is_none(), "nothing changed -> no bbox");
        assert!(!diff.size_mismatch);
        let mask = image::load_from_memory(&diff.mask_png).unwrap().to_rgba8();
        assert!(mask.pixels().all(|p| p.0 == [0, 0, 0, 255]), "an all-identical diff mask must be all-black");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_one_pixel_change_is_detected_with_a_tight_bbox_and_a_small_percentage() {
        let d = scratch("onepixel");
        let a = save_png(&d, "a.png", 10, 10, [0, 0, 0]);
        let f = d.join("b.png");
        let mut img = image::RgbImage::from_pixel(10, 10, image::Rgb([0, 0, 0]));
        img.put_pixel(3, 7, image::Rgb([255, 255, 255]));
        img.save(&f).unwrap();
        let diff = diff_images(&a, &f).unwrap();
        assert_eq!(diff.changed_pixels, 1);
        assert_eq!(diff.total_pixels, 100);
        assert_eq!(diff.percent_different, 1.0);
        assert_eq!(diff.bbox, Some(DiffBBox { x: 3, y: 7, width: 1, height: 1 }));
        assert!(!diff.size_mismatch);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn fully_different_images_are_close_to_a_hundred_percent() {
        let d = scratch("fulldiff");
        let a = save_png(&d, "a.png", 8, 8, [0, 0, 0]);
        let b = save_png(&d, "b.png", 8, 8, [255, 255, 255]);
        let diff = diff_images(&a, &b).unwrap();
        assert_eq!(diff.changed_pixels, 64);
        assert_eq!(diff.percent_different, 100.0);
        assert_eq!(diff.bbox, Some(DiffBBox { x: 0, y: 0, width: 8, height: 8 }));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn differing_dimensions_are_aligned_and_flagged_without_panicking() {
        let d = scratch("sizemismatch");
        let a = save_png(&d, "a.png", 4, 4, [50, 50, 50]);
        let b = save_png(&d, "b.png", 6, 6, [50, 50, 50]); // same color, bigger canvas
        let diff = diff_images(&a, &b).unwrap();
        assert!(diff.size_mismatch);
        assert_eq!((diff.width, diff.height), (6, 6), "aligned to the union bounding box");
        // The 4x4 overlap matches; the padding strip (6x6 minus 4x4) has no counterpart -> all changed.
        assert_eq!(diff.changed_pixels, 36 - 16);
        assert!(diff.bbox.is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_garbage_file_is_an_error_not_a_panic() {
        let d = scratch("garbage");
        let a = save_png(&d, "a.png", 4, 4, [1, 2, 3]);
        let garbage = d.join("garbage.png");
        std::fs::write(&garbage, b"not an image at all").unwrap();
        assert!(diff_images(&a, &garbage).is_err());
        assert!(diff_images(&garbage, &a).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_truncated_file_is_an_error_not_a_panic() {
        let d = scratch("truncated");
        let a = save_png(&d, "a.png", 4, 4, [1, 2, 3]);
        // A real PNG's signature + IHDR chunk with the rest of the file (IDAT/pixel data) cut off.
        let full = std::fs::read(&a).unwrap();
        let truncated = d.join("truncated.png");
        std::fs::write(&truncated, &full[..full.len().min(16)]).unwrap();
        assert!(diff_images(&a, &truncated).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        let d = scratch("missing");
        let a = save_png(&d, "a.png", 4, 4, [1, 2, 3]);
        assert!(diff_images(&a, &d.join("does-not-exist.png")).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// CPE-1490: a real, validly-decodable image whose width exceeds `MAX_DIFF_DIMENSION` must be
    /// refused, bounded, before any diff/mask buffer is built — the diff-specific cap layered on top of
    /// `decode_thumb_image`'s own (much looser) 20,000-px decode bound. A tall-and-narrow shape (only
    /// the width breaches the cap) keeps the fixture's real pixel data small and the test fast, while
    /// still genuinely exceeding `MAX_DIFF_DIMENSION` — this is not a header-only/bomb fixture, the
    /// image really does decode successfully; `diff_images` must still refuse it.
    #[test]
    fn an_oversized_dimension_image_is_refused_before_diffing() {
        let d = scratch("oversize");
        let over = MAX_DIFF_DIMENSION + 1;
        let a = save_png(&d, "a.png", over, 2, [9, 9, 9]);
        let b = save_png(&d, "b.png", 4, 4, [9, 9, 9]);
        let err = diff_images(&a, &b).unwrap_err();
        assert!(err.contains("too large"), "expected a bounded refusal, got: {err}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
