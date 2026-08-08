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
//!
//! Reference-cycle guard (CPE-1414, epic CPE-718): the depth guard above stops literal element
//! *nesting* from overflowing the stack, but `usvg` resolves `<use>`/`<symbol>` references by cloning
//! the referenced subtree, and that resolution is *recursive with no small-stack-safe cap of its own*.
//! usvg special-cases only a **direct** self-reference (`<use href="#self">`) and one-hop back-references;
//! a **2+-hop mutual cycle** (`<symbol id="a"><use href="#b"/></symbol>` +
//! `<symbol id="b"><use href="#a"/></symbol>`) falls through to usvg's `depth > 1024` cap, whose own
//! per-level stack cost overflows a small (256KB) thread stack well before 1024. [`svg_use_reference_cycle`]
//! is a second non-recursive pre-scan — a **graph** guard rather than the depth guard's counter — that
//! builds the `<use>` reference graph (edge `S -> T` whenever a `<use href="#T">` appears anywhere inside
//! the subtree of an id'd element `S`, the exact "cloning `S` re-clones `T`" relation usvg would recurse
//! on) and rejects the document iff that graph contains a cycle, detected with an explicit-stack (never
//! call-stack) three-colour DFS. It is deliberately narrow — it only follows `<use>` references, so a
//! legitimate SVG that reuses `<use>`/`<symbol>` heavily but *acyclically* (even a deep reference chain)
//! is never rejected — only an actual cycle is. Crucially, the graph's strings are sourced from the **same
//! `roxmltree` parse usvg itself uses** (mirroring usvg's SVGZ-decompress + UTF-8 preprocessing and its
//! `allow_dtd: true` parsing option), so the guard resolves numeric char refs, predefined entities, and
//! internal-subset DTD entities *identically* to usvg — closing the whole class of "the guard modelled a
//! different decoded string than usvg resolves" bypasses (`&#35;b`, `<!ENTITY r "#b">`) that successive
//! hand-rolled byte scans kept losing to. See [`svg_use_reference_cycle`].

use image::{DynamicImage, RgbaImage};
use std::collections::HashMap;

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

/// Non-recursive guard against a `<use>`/`<symbol>` **reference cycle**, run before the document is
/// handed to `usvg` (see the module doc comment). `usvg` resolves a `<use href="#T">` by recursively
/// cloning `T`'s subtree; a 2+-hop mutual cycle overflows a small stack because usvg only special-cases a
/// direct self-reference. Returns `true` iff the reference graph has a cycle (or the document can't be
/// safely inspected — see below), so `rasterize_svg` rejects it before usvg ever recurses.
///
/// **Sourced from `roxmltree`, exactly like usvg.** Earlier hand-rolled byte scans kept losing to the same
/// class of bug — the guard modelling a *different* decoded string than usvg resolves (numeric char refs
/// `&#35;`, then internal-subset DTD entities `<!ENTITY r "#b">`). This version instead mirrors
/// `usvg::Tree::from_data`'s own preprocessing (gzip-decompress an SVGZ payload, require UTF-8) and parses
/// with the very same parser+options usvg uses — `roxmltree::Document::parse_with_options(.., allow_dtd:
/// true)` — then extracts the `<use>` reference graph from that parsed tree. Because roxmltree resolves
/// numeric refs, the predefined entities, **and** internal-subset DTD entities identically for both, the
/// graph is keyed on the exact strings usvg matches, closing the whole "guard sees a different string"
/// divergence class in one move (`roxmltree` is already usvg's transitive XML parser — no new dependency
/// surface; it's pinned to the same version usvg resolves).
///
/// For each element carrying an `id`, and each `<use>` (any namespace) with an `href`/`xlink:href` of the
/// form `#T`, records an edge from every id'd ancestor **and** the `<use>`'s own id (its own node is the
/// first entry `Node::ancestors()` yields) to `T` — precisely the "cloning that ancestor re-clones `T`"
/// relation usvg recurses on. Only `<use>` references become edges, so acyclic reuse of `<use>`/`<symbol>`
/// — however heavy — never trips it. The cycle test itself is an explicit-stack three-colour DFS
/// ([`graph_has_cycle`]), never the call stack.
///
/// **Failure is safe:** if the SVGZ decompress, the UTF-8 check, or the roxmltree parse fails, usvg's
/// *identical* parse would fail too, so the guard falls through (`false`) and lets usvg report the real
/// error — the only path that reaches usvg's dangerous `<use>` recursion is one where the parse *succeeds*,
/// and on that path this guard has already seen the same tree and rejected any real cycle. As a final
/// belt-and-braces measure the depth guard is re-run here on the *decompressed* bytes (an SVGZ payload's
/// nesting is invisible to `rasterize_svg`'s pre-scan of the still-compressed bytes), so this guard's own
/// `roxmltree` parse can never be walked into a deep-nesting overflow either.
fn svg_use_reference_cycle(bytes: &[u8]) -> bool {
    fn intern<'a>(ids: &mut HashMap<&'a str, usize>, adj: &mut Vec<Vec<usize>>, key: &'a str) -> usize {
        if let Some(&idx) = ids.get(key) {
            return idx;
        }
        let idx = adj.len();
        ids.insert(key, idx);
        adj.push(Vec::new());
        idx
    }

    // Mirror `usvg::Tree::from_data`: an SVGZ (gzip) payload is decompressed first (some `.svg` files are
    // actually gzipped, and usvg handles them, so a cycle hidden inside one must be inspected here too).
    let decompressed;
    let data: &[u8] = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = flate2::read::GzDecoder::new(bytes);
        let mut buf = Vec::new();
        if std::io::Read::read_to_end(&mut decoder, &mut buf).is_err() {
            return false; // usvg's own `decompress_svgz` fails identically -> let it report the error
        }
        decompressed = buf;
        &decompressed
    } else {
        bytes
    };

    // `roxmltree` recurses per nesting level, so guard this guard's own parse against a deep-nesting
    // overflow the same way `rasterize_svg` guards usvg's. Needed here for the SVGZ path (where
    // `rasterize_svg`'s pre-scan ran on the still-compressed bytes); redundant-but-cheap otherwise.
    if xml_nesting_too_deep(data, MAX_SVG_NESTING_DEPTH) {
        return true;
    }

    let text = match std::str::from_utf8(data) {
        Ok(t) => t,
        Err(_) => return false, // usvg's `from_utf8` fails identically (NotAnUtf8Str) -> let it report
    };
    // The SAME parser + option usvg uses (`allow_dtd: true` is what makes roxmltree expand internal-subset
    // DTD entities — the exact resolution the last review's bypass relied on).
    let xml_opt = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = match roxmltree::Document::parse_with_options(text, xml_opt) {
        Ok(d) => d,
        Err(_) => return false, // usvg's identical parse fails too (incl. roxmltree's entity-bomb limit)
    };

    // Build the `<use>` reference graph from the parsed tree. roxmltree's `attribute()`/`value()` return
    // already entity/DTD-decoded strings, so `strip_prefix('#')` on them matches what usvg resolves.
    let mut ids: HashMap<&str, usize> = HashMap::new();
    let mut adj: Vec<Vec<usize>> = Vec::new();
    for node in doc.descendants() {
        if !node.is_element() || node.tag_name().name() != "use" {
            continue;
        }
        // The `href`/`xlink:href` attribute, whatever its namespace prefix, by local name.
        let Some(href) = node.attributes().find(|a| a.name() == "href").map(|a| a.value()) else {
            continue;
        };
        let Some(target_id) = href.trim().strip_prefix('#').filter(|s| !s.is_empty()) else {
            continue; // external / non-fragment ref: usvg can't recurse a cycle through it
        };
        let target = intern(&mut ids, &mut adj, target_id);
        // Edge from every id'd ancestor — INCLUDING this `<use>`'s own id, since `ancestors()` yields the
        // node itself first — to the referenced target.
        for anc in node.ancestors() {
            if let Some(src_id) = anc.attribute("id") {
                let src = intern(&mut ids, &mut adj, src_id);
                adj[src].push(target);
            }
        }
    }

    graph_has_cycle(&adj)
}

/// Iterative (explicit-stack, **never** call-stack) three-colour DFS cycle test over adjacency list
/// `adj`. Returns `true` iff the directed graph contains a cycle (including a self-loop). Sized linearly
/// in nodes+edges and uses only heap storage, so it can't stack-overflow however adversarial the graph.
fn graph_has_cycle(adj: &[Vec<usize>]) -> bool {
    // 0 = unvisited, 1 = on the current DFS path (grey), 2 = fully explored (black).
    let mut colour = vec![0u8; adj.len()];
    for start in 0..adj.len() {
        if colour[start] != 0 {
            continue;
        }
        colour[start] = 1;
        let mut work: Vec<(usize, usize)> = vec![(start, 0)];
        while let Some(&mut (node, ref mut next)) = work.last_mut() {
            if *next < adj[node].len() {
                let child = adj[node][*next];
                *next += 1;
                match colour[child] {
                    0 => {
                        colour[child] = 1;
                        work.push((child, 0));
                    }
                    1 => return true, // back-edge to a grey node -> cycle (a self-loop hits this too)
                    _ => {}
                }
            } else {
                colour[node] = 2;
                work.pop();
            }
        }
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
    if svg_use_reference_cycle(bytes) {
        return Err("SVG <use>/<symbol> reference cycle".to_string());
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

    /// An SVG whose `<use>`/`<symbol>` references form an N-node ring (a cycle):
    /// `s0 -> s1 -> ... -> s(N-1) -> s0`.
    fn cyclic_use_chain_svg(n: usize) -> Vec<u8> {
        let mut s = String::from(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">"#,
        );
        for k in 0..n {
            let next = (k + 1) % n;
            s.push_str(&format!(r##"<symbol id="s{k}"><use xlink:href="#s{next}"/></symbol>"##));
        }
        s.push_str(r##"<use xlink:href="#s0"/>"##);
        s.push_str("</svg>");
        s.into_bytes()
    }

    /// An SVG with a deep but strictly ACYCLIC `<use>` reference chain: a base shape, then `depth`
    /// `<use>`s each referencing the previous one. Heavy `<use>` reuse with no cycle — must NOT be
    /// flagged (the false-positive guard).
    fn acyclic_use_chain_svg(depth: usize) -> Vec<u8> {
        let mut s = String::from(
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10"><rect id="u0" width="4" height="4" fill="#f00"/>"##,
        );
        for k in 1..=depth {
            s.push_str(&format!(r##"<use id="u{k}" xlink:href="#u{prev}"/>"##, prev = k - 1));
        }
        s.push_str(&format!(r##"<use xlink:href="#u{depth}"/>"##));
        s.push_str("</svg>");
        s.into_bytes()
    }

    #[test]
    fn use_cycle_guard_flags_a_two_hop_mutual_cycle() {
        assert!(svg_use_reference_cycle(&cyclic_use_chain_svg(2)), "2-hop mutual <use> cycle must be flagged");
    }

    #[test]
    fn use_cycle_guard_flags_a_three_hop_cycle() {
        assert!(svg_use_reference_cycle(&cyclic_use_chain_svg(3)), "3-hop <use> cycle must be flagged");
    }

    #[test]
    fn use_cycle_guard_flags_a_direct_self_reference() {
        // `<use id="self" href="#self">` — a self-loop; already skipped by usvg, but our graph guard
        // rejects it outright (safe: degrades to the "no thumbnail" path). Confirms the self-loop case.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <use id="self" xlink:href="#self"/>
        </svg>"##;
        assert!(svg_use_reference_cycle(svg), "a direct <use> self-reference is a cycle");
    }

    #[test]
    fn use_cycle_guard_allows_deep_acyclic_reuse() {
        // A deep acyclic reference chain and ordinary reuse of a shared symbol must NOT be flagged —
        // this is the over-blocking / false-positive guard: legitimate SVGs reuse <use>/<symbol> heavily.
        assert!(!svg_use_reference_cycle(&acyclic_use_chain_svg(200)), "deep acyclic <use> reuse must render");
        // A diamond: two users of one shared symbol (no cycle).
        let diamond = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <symbol id="leaf"><rect width="2" height="2" fill="#0f0"/></symbol>
            <symbol id="a"><use xlink:href="#leaf"/></symbol>
            <symbol id="b"><use xlink:href="#leaf"/></symbol>
            <use xlink:href="#a"/><use xlink:href="#b"/>
        </svg>"##;
        assert!(!svg_use_reference_cycle(diamond), "acyclic reuse of a shared symbol must not be flagged");
        // A plain SVG with no <use> at all.
        assert!(!svg_use_reference_cycle(&square_svg(10, 10)), "an SVG with no references has no cycle");
    }

    #[test]
    fn use_cycle_guard_is_quote_aware_and_not_fooled_by_a_gt_in_an_attribute_value() {
        // The cycle-forming `<use>`s are preceded by a decoy element whose attribute value contains a
        // literal `/>` — a quote-unaware scan (the CPE-1398 follow-up bypass class) could miscount the tag
        // boundary and skip the real edges. The guard must still see the a<->b cycle.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <rect id="decoy" data-x="/>" width="1" height="1"/>
            <symbol id="a"><use xlink:href="#b"/></symbol>
            <symbol id="b"><use xlink:href="#a"/></symbol>
            <use xlink:href="#a"/>
        </svg>"##;
        assert!(svg_use_reference_cycle(svg), "an embedded '>' in an attribute must not hide the cycle");
    }

    #[test]
    fn use_cycle_guard_decodes_entities_before_matching_references() {
        // The confirmed CPE-1414 review bypass: hrefs written as numeric/hex char refs (`&#35;` / `&#x23;`
        // = `#`) decode to `#b`/`#a` under roxmltree BEFORE usvg resolves them, forming the a<->b cycle —
        // but a raw-byte scan sees a leading `&`, not `#`, and misses the edge. The guard must decode first.
        let numeric = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <symbol id="a"><use xlink:href="&#35;b"/></symbol>
            <symbol id="b"><use xlink:href="&#35;a"/></symbol>
            <use xlink:href="#a"/>
        </svg>"##;
        assert!(svg_use_reference_cycle(numeric), "a numeric-char-ref-encoded (&#35;) <use> cycle must be flagged");

        let hex = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <symbol id="a"><use xlink:href="&#x23;b"/></symbol>
            <symbol id="b"><use xlink:href="&#x23;a"/></symbol>
            <use xlink:href="#a"/>
        </svg>"##;
        assert!(svg_use_reference_cycle(hex), "a hex-char-ref-encoded (&#x23;) <use> cycle must be flagged");

        // Entity-encoded *id* (`&#97;` = `a`) must intern to the same node key the plain `#a` href targets.
        let entity_id = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <symbol id="&#97;"><use xlink:href="#b"/></symbol>
            <symbol id="b"><use xlink:href="#a"/></symbol>
            <use xlink:href="#a"/>
        </svg>"##;
        assert!(svg_use_reference_cycle(entity_id), "an entity-encoded id must not dodge the cycle guard");
    }

    #[test]
    fn use_cycle_guard_flags_an_internal_subset_dtd_entity_cycle() {
        // The second confirmed CPE-1414 review bypass: hrefs written as internal-subset DTD entities
        // (`<!ENTITY r "#b">` -> `&r;` = `#b`). A hand-rolled decoder can't know these custom entities, but
        // roxmltree (with `allow_dtd: true`, exactly as usvg parses) expands them, so sourcing the graph
        // from roxmltree matches usvg and the a<->b cycle is seen.
        let dtd = br##"<?xml version="1.0"?>
        <!DOCTYPE svg [<!ENTITY r "#b"><!ENTITY q "#a">]>
        <svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <symbol id="a"><use xlink:href="&r;"/></symbol>
            <symbol id="b"><use xlink:href="&q;"/></symbol>
            <use xlink:href="#a"/>
        </svg>"##;
        assert!(svg_use_reference_cycle(dtd), "an internal-subset DTD-entity <use> cycle must be flagged");
    }

    #[test]
    fn use_cycle_guard_falls_through_on_unparseable_input() {
        // If roxmltree can't parse it, usvg's identical parse fails too, so the guard reports "no cycle"
        // and lets usvg surface the real error — it must not panic or spuriously flag a cycle.
        assert!(!svg_use_reference_cycle(b"<svg><this is not valid xml"));
        assert!(!svg_use_reference_cycle(&[0xff, 0xfe, 0x00, 0x3c])); // not UTF-8
        assert!(!svg_use_reference_cycle(b""));
    }

    #[test]
    fn use_cycle_guard_allows_acyclic_entity_encoded_reference() {
        // Decoding must not OVER-reject: a legit acyclic href written as a numeric char ref to a real,
        // non-cyclic target has no cycle and must not be flagged.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <symbol id="leaf"><rect width="2" height="2" fill="#00f"/></symbol>
            <use xlink:href="&#35;leaf"/>
        </svg>"##;
        assert!(!svg_use_reference_cycle(svg), "an acyclic entity-encoded reference must not be flagged");
    }

    #[test]
    fn use_cycle_guard_handles_truncated_and_malformed_input_without_panicking() {
        for doc in [
            "<use xlink:href=\"#a\"",
            "<symbol id=",
            "<symbol id=\"a\"><use xlink:href=\"#",
            "<use href=\"#\"/>", // empty fragment -> no edge
            "<use href=\"http://example.com/x.svg#a\"/>", // external ref -> ignored (no '#'-local edge)
            "",
        ] {
            let _ = svg_use_reference_cycle(doc.as_bytes());
        }
    }

    #[test]
    fn rasterize_svg_rejects_a_use_reference_cycle_gracefully() {
        let err = rasterize_svg(&cyclic_use_chain_svg(2), 32);
        assert!(err.is_err(), "a mutual <use> reference cycle must be rejected, not risk a stack overflow");
    }

    #[test]
    fn rasterize_svg_renders_deep_acyclic_reuse() {
        // The false-positive guard end-to-end: deep acyclic reuse must still produce an image.
        let img = rasterize_svg(&acyclic_use_chain_svg(200), 32);
        assert!(img.is_ok(), "deep acyclic <use> reuse must still render, got {img:?}");
    }

    #[test]
    fn rasterize_svg_renders_acyclic_entity_encoded_reference() {
        // Decoding entities must not make the guard over-reject a legitimate acyclic entity-encoded href.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <rect id="leaf" width="8" height="8" fill="#00f"/>
            <use xlink:href="&#35;leaf"/>
        </svg>"##;
        let img = rasterize_svg(svg, 32);
        assert!(img.is_ok(), "a legit acyclic entity-encoded reference must still render, got {img:?}");
    }
}
