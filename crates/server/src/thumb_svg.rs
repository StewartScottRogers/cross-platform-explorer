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
//!
//! Stack-overflow guard (CPE-1413, mirrors `cpe_webdav`'s `MAX_XML_NESTING_DEPTH`/`xml_nesting_too_deep`
//! fix for CPE-1398 — the exact same underlying vulnerability class): `usvg::Tree::from_data` parses the
//! raw SVG text into a DOM via `roxmltree::Document::parse` *before* usvg's own element-count/`use`-depth
//! caps ever run, and — like most XML parsers — `roxmltree` recurses per nesting level with **no depth
//! limit of its own**, so a crafted `<g><g><g>...` a few thousand deep is enough to blow a thread stack
//! and crash the whole process with an **uncatchable** stack overflow, confirmed empirically here exactly
//! as it was for webdav's PROPFIND parsing. [`xml_nesting_too_deep`] is the same non-recursive,
//! quote/comment/CDATA/PI/DOCTYPE-aware byte scan run before the bytes are ever handed to `usvg`, capped
//! at [`MAX_SVG_NESTING_DEPTH`]. Written by hand instead of reusing webdav's `xmlparser`-crate scanner
//! because `rasterize_svg` takes raw `&[u8]` (not `&str`), so this version needs no UTF-8 boundary
//! handling at all — a further reason to keep it a self-contained byte scan rather than adding a new
//! dependency for it.

use image::{DynamicImage, RgbaImage};

/// Same spirit as `thumb_source::MAX_IMAGE_DIMENSION` — an SVG's *declared* intrinsic size is
/// clamped to this before we ever allocate a canvas. Real SVG artwork is never this big.
const MAX_SVG_DIMENSION: u32 = 20_000;

/// The deepest element nesting [`xml_nesting_too_deep`] will allow before `rasterize_svg` refuses the
/// document outright (CPE-1413). Mirrors `cpe_webdav::MAX_XML_NESTING_DEPTH`'s reasoning and value: the
/// exact crash depth is stack-size- and build-profile-dependent (confirmed here at well under 500 levels
/// on a 256KB debug-build thread stack — the same small-stack probe CPE-1398's fix was validated
/// against), so 64 is sized with a wide margin under any observed crash depth while a real hand-authored
/// or tool-exported SVG's group nesting is essentially always under a few dozen levels, so this costs
/// nothing for legitimate artwork.
const MAX_SVG_NESTING_DEPTH: usize = 64;

/// Cheap, non-recursive guard against maliciously (or accidentally) deep XML nesting, run before the
/// document is handed to `usvg`/`roxmltree` (see the module doc comment and [`MAX_SVG_NESTING_DEPTH`]).
///
/// Walks `bytes` once, tracking only a `depth: usize` counter — never the real call stack — so this
/// itself can't stack-overflow no matter how deep or malformed the input is. Quote-aware when scanning
/// for a tag's closing `>` (an attribute value containing a literal `>`, e.g. `<a b="/>">`, is legal XML
/// and must not be misread as the tag's own close — this is the exact scan-evasion bug class a first
/// version of webdav's equivalent guard fell to in its CPE-1398 follow-up) and skips comments, CDATA
/// sections, processing instructions, and `<!DOCTYPE ...>` declarations (none of which nest as elements)
/// without counting them toward depth.
fn xml_nesting_too_deep(bytes: &[u8], max_depth: usize) -> bool {
    fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
        if from > bytes.len() {
            return None;
        }
        bytes[from..].windows(needle.len()).position(|w| w == needle).map(|p| from + p)
    }
    fn starts_with(bytes: &[u8], i: usize, needle: &[u8]) -> bool {
        bytes.len() >= i + needle.len() && &bytes[i..i + needle.len()] == needle
    }

    let n = bytes.len();
    let mut i = 0usize;
    let mut depth: usize = 0;
    while i < n {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if starts_with(bytes, i, b"<!--") {
            // Comment: skip to "-->" without counting anything inside it.
            match find(bytes, i + 4, b"-->") {
                Some(end) => i = end + 3,
                None => break, // unterminated — let usvg's real parser report the error
            }
            continue;
        }
        if starts_with(bytes, i, b"<![CDATA[") {
            match find(bytes, i + 9, b"]]>") {
                Some(end) => i = end + 3,
                None => break,
            }
            continue;
        }
        if starts_with(bytes, i, b"<?") {
            // Processing instruction / XML declaration.
            match find(bytes, i + 2, b"?>") {
                Some(end) => i = end + 2,
                None => break,
            }
            continue;
        }
        if i + 1 < n && bytes[i + 1] == b'!' {
            // `<!DOCTYPE ...>` (or similar): not an element, so it never affects depth. Honor a nested
            // `[...]` internal subset and quoted values so an embedded '>' inside either doesn't end the
            // declaration early.
            let mut j = i + 2;
            let mut bracket_depth: i32 = 0;
            let mut quote: Option<u8> = None;
            while j < n {
                let c = bytes[j];
                if let Some(q) = quote {
                    if c == q {
                        quote = None;
                    }
                } else if c == b'"' || c == b'\'' {
                    quote = Some(c);
                } else if c == b'[' {
                    bracket_depth += 1;
                } else if c == b']' {
                    bracket_depth -= 1;
                } else if c == b'>' && bracket_depth <= 0 {
                    break;
                }
                j += 1;
            }
            if j >= n {
                break;
            }
            i = j + 1;
            continue;
        }

        // A real start/end/empty-element tag: scan to the first UNQUOTED '>'.
        let is_end_tag = i + 1 < n && bytes[i + 1] == b'/';
        let mut j = i + 1;
        let mut quote: Option<u8> = None;
        while j < n {
            let c = bytes[j];
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
            } else if c == b'"' || c == b'\'' {
                quote = Some(c);
            } else if c == b'>' {
                break;
            }
            j += 1;
        }
        if j >= n {
            break; // unterminated tag — let usvg's real parser report the error
        }
        let self_closing = j > i + 1 && bytes[j - 1] == b'/';
        if is_end_tag {
            depth = depth.saturating_sub(1);
        } else if !self_closing {
            depth += 1;
            if depth > max_depth {
                return true;
            }
        }
        i = j + 1;
    }
    false
}

/// Rasterize `bytes` (the contents of an `.svg` file) to an RGBA image whose longest edge is at most
/// `max_edge` pixels, preserving the document's aspect ratio. Never panics: a malformed document, an
/// implausible declared size, an implausibly deep element nesting, or a render-target allocation failure
/// all return `Err`.
pub fn rasterize_svg(bytes: &[u8], max_edge: u32) -> Result<DynamicImage, String> {
    if xml_nesting_too_deep(bytes, MAX_SVG_NESTING_DEPTH) {
        return Err("SVG element nesting too deep".to_string());
    }

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

    /// An SVG whose `<g>` nesting is `depth` levels deep (CPE-1413's stack-overflow probe).
    fn deeply_nested_svg(depth: usize) -> Vec<u8> {
        let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
        s.push_str(&"<g>".repeat(depth));
        s.push_str(r##"<rect width="10" height="10" fill="#f00"/>"##);
        s.push_str(&"</g>".repeat(depth));
        s.push_str("</svg>");
        s.into_bytes()
    }

    #[test]
    fn xml_nesting_guard_rejects_deep_nesting_and_allows_shallow_real_artwork() {
        assert!(xml_nesting_too_deep(&deeply_nested_svg(4000), MAX_SVG_NESTING_DEPTH));
        assert!(!xml_nesting_too_deep(&square_svg(10, 10), MAX_SVG_NESTING_DEPTH));
        // A realistic, moderately-nested illustration (well under the cap) must not be rejected.
        assert!(!xml_nesting_too_deep(&deeply_nested_svg(20), MAX_SVG_NESTING_DEPTH));
    }

    #[test]
    fn xml_nesting_guard_is_quote_aware_and_not_fooled_by_a_literal_gt_in_an_attribute_value() {
        // `<a b="/>">` is legal XML whose attribute value contains the literal bytes `/>` — a
        // quote-UNaware scan could misread that embedded '>' as the tag's own close and wrongly treat
        // the tag as self-closing, silently under-counting depth (the exact CPE-1398 follow-up bypass
        // class). Confirm this guard is not fooled: it must still recognize deep nesting built from this
        // shape as too deep.
        let n = 4000;
        let bypass_shape = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\">{}{}</svg>",
            "<a b=\"/>\">".repeat(n),
            "</a>".repeat(n)
        );
        assert!(xml_nesting_too_deep(bypass_shape.as_bytes(), MAX_SVG_NESTING_DEPTH));
    }

    #[test]
    fn xml_nesting_guard_ignores_comments_cdata_and_processing_instructions() {
        // A literal '>' inside a comment/CDATA/PI must not be misread as closing a tag, and none of
        // these constructs should themselves count toward depth.
        let doc = concat!(
            "<?xml version=\"1.0\"?>",
            "<svg xmlns=\"http://www.w3.org/2000/svg\">",
            "<!-- a comment with a lone > inside -->",
            "<![CDATA[ some > data ]]>",
            "<rect width=\"1\" height=\"1\"/>",
            "</svg>",
        );
        assert!(!xml_nesting_too_deep(doc.as_bytes(), MAX_SVG_NESTING_DEPTH));
    }

    #[test]
    fn xml_nesting_guard_handles_truncated_and_malformed_input_without_panicking() {
        // Unterminated comment/CDATA/PI/DOCTYPE/tag, and various truncation points — none of these must
        // panic or loop forever; the real usvg parse is left to report the actual error.
        for doc in [
            "<",
            "<!",
            "<!-",
            "<!--unterminated comment",
            "<![CDATA[unterminated",
            "<?xml unterminated",
            "<!DOCTYPE unterminated",
            "<svg><g><g><g>",
            "",
        ] {
            xml_nesting_too_deep(doc.as_bytes(), MAX_SVG_NESTING_DEPTH);
        }
    }

    #[test]
    fn rasterize_svg_rejects_deeply_nested_groups_gracefully() {
        // Post-CPE-1413-fix: this must return a graceful Err, not panic (and, per the accompanying
        // integration test, not stack-overflow even on a small thread stack).
        let err = rasterize_svg(&deeply_nested_svg(4000), 32);
        assert!(err.is_err(), "implausibly deep SVG group nesting must be rejected, not risk a stack overflow");
    }
}
