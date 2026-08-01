//! SVG thumbnail rasterization (CPE-1236, epic CPE-718): render an `.svg` file to an
//! `image::DynamicImage` at (approximately) `max_edge` pixels on its longest side, so the thumbnail
//! grid shows the actual vector artwork instead of a generic icon. Integrates into the same
//! [`crate::thumb_source::decode_thumb_image`] dispatch the raster/PSD paths use — the caller's usual
//! orient+downscale+encode steps then run unchanged (SVG carries no EXIF, so orientation is a no-op,
//! and the image is already ~`max_edge` sized so the final `.thumbnail()` call in `thumbnail.rs` is
//! close to a no-op).
//!
//! Uses `resvg` (which re-exports `usvg` for XML -> scene-tree parsing and `tiny-skia` for the actual
//! rasterization) — the lightest widely-used pure-Rust SVG stack, and the one named explicitly in the
//! ticket, rather than a browser engine or a system SVG library. `Options::default()`'s `fontdb` is an
//! *empty* database (no automatic system-font scan — we don't enable resvg's `system-fonts` feature),
//! so `<text>` elements render without glyphs rather than touching the filesystem: deterministic and
//! headless-safe across the 3-OS CI matrix. Malformed SVG (bad XML, no root `<svg>`, non-positive
//! size) returns an `Err`, which the thumbnail pipeline already treats as "no thumbnail, show the type
//! icon" — never a panic.
//!
//! Bomb-guard (mirrors `thumb_source`'s PNG/PSD decompression-bomb guard, CPE-1087): an SVG's
//! *declared* intrinsic size (from `width`/`height`/`viewBox`) is clamped before any canvas is
//! allocated, so a crafted `viewBox="0 0 999999999 999999999"` can't force a multi-gigabyte pixmap.
//! `usvg` itself also caps the parsed element count (1,000,000) as a second, independent guard.

use image::{DynamicImage, RgbaImage};

/// Same spirit as `thumb_source::MAX_IMAGE_DIMENSION` — an SVG's *declared* intrinsic size is
/// clamped to this before we ever allocate a canvas. Real SVG artwork is never this big.
const MAX_SVG_DIMENSION: u32 = 20_000;

/// Rasterize `bytes` (the contents of an `.svg` file) to an RGBA image whose longest edge is at most
/// `max_edge` pixels, preserving the document's aspect ratio. Never panics: a malformed document, an
/// implausible declared size, or a render-target allocation failure all return `Err`.
pub fn rasterize_svg(bytes: &[u8], max_edge: u32) -> Result<DynamicImage, String> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(bytes, &opt).map_err(|e| e.to_string())?;

    let size = tree.size();
    let (src_w, src_h) = (size.width(), size.height());
    if src_w > MAX_SVG_DIMENSION as f32 || src_h > MAX_SVG_DIMENSION as f32 {
        return Err(format!(
            "SVG intrinsic size {src_w}x{src_h} exceeds the {MAX_SVG_DIMENSION}px bomb-guard limit"
        ));
    }

    let edge = max_edge.max(1) as f32;
    let scale = (edge / src_w).min(edge / src_h);
    let out_w = (src_w * scale).round().max(1.0) as u32;
    let out_h = (src_h * scale).round().max(1.0) as u32;
    if out_w > MAX_SVG_DIMENSION || out_h > MAX_SVG_DIMENSION {
        return Err("SVG scaled render target exceeds the bomb-guard limit".to_string());
    }

    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(out_w, out_h).ok_or("could not allocate an SVG render target")?;
    let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // tiny-skia stores premultiplied alpha; `image`/PNG expect straight alpha, so demultiply per pixel.
    let mut raw = Vec::with_capacity(pixmap.pixels().len() * 4);
    for px in pixmap.pixels() {
        let c = px.demultiply();
        raw.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let rgba = RgbaImage::from_raw(out_w, out_h, raw).ok_or("SVG render buffer size mismatch")?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small square SVG (explicit `width`/`height`) filled solid red.
    fn square_svg(w: u32, h: u32) -> Vec<u8> {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}">
                 <rect width="{w}" height="{h}" fill="#ff0000"/>
               </svg>"##
        )
        .into_bytes()
    }

    #[test]
    fn rasterize_svg_produces_a_non_empty_png_at_the_expected_max_edge() {
        // 100x40 (wide) -> longest edge (width) scaled to 32.
        let img = rasterize_svg(&square_svg(100, 40), 32).unwrap();
        assert_eq!(img.width(), 32, "longest edge scaled to max_edge");
        assert!(img.height() <= 32 && img.height() >= 10, "aspect preserved: {}", img.height());
        // Non-empty: at least one solid, mostly-opaque red pixel.
        let rgba = img.to_rgba8();
        let center = *rgba.get_pixel(img.width() / 2, img.height() / 2);
        assert!(center.0[3] > 200, "center pixel should be opaque, got {:?}", center.0);
        assert!(center.0[0] > 200 && center.0[1] < 50, "center pixel should be red, got {:?}", center.0);
    }

    #[test]
    fn rasterize_svg_preserves_aspect_for_a_tall_document() {
        let img = rasterize_svg(&square_svg(20, 100), 32).unwrap();
        assert_eq!(img.height(), 32, "longest edge (height) scaled to max_edge");
        assert!(img.width() <= 32 && img.width() >= 4, "aspect preserved: {}", img.width());
    }

    #[test]
    fn rasterize_svg_rejects_malformed_xml_without_panicking() {
        let err = rasterize_svg(b"<svg><this is not valid xml", 32);
        assert!(err.is_err(), "malformed SVG must fall back gracefully, not panic");
    }

    #[test]
    fn rasterize_svg_rejects_a_document_with_no_root_svg_element() {
        let err = rasterize_svg(b"<html><body>not an svg</body></html>", 32);
        assert!(err.is_err(), "a non-SVG XML document must be rejected");
    }

    #[test]
    fn rasterize_svg_bomb_guards_an_oversized_declared_viewbox() {
        // A tiny XML payload declaring a canvas far past MAX_SVG_DIMENSION.
        let bomb = br##"<svg xmlns="http://www.w3.org/2000/svg" width="999999999" height="999999999">
                          <rect width="100%" height="100%" fill="#000"/>
                        </svg>"##;
        let err = rasterize_svg(bomb, 32);
        assert!(err.is_err(), "an implausibly huge declared SVG size must be rejected, not OOM/panic");
    }

    #[test]
    fn rasterize_svg_handles_a_zero_max_edge_without_panicking() {
        // max_edge=0 must clamp to at least 1px rather than dividing by zero / allocating a 0x0 pixmap.
        let img = rasterize_svg(&square_svg(10, 10), 0).unwrap();
        assert_eq!((img.width(), img.height()), (1, 1));
    }
}
