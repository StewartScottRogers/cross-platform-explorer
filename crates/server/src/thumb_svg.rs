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
//! `<use>` reference-chain guard (CPE-1437, closes the small-stack bar CPE-1414 left open): a **flat,
//! acyclic** sibling chain of `<use>` elements each referencing the previous (`#u1` <- `#u2` <- ... <-
//! `#u500`) is only ~1 level deep in the raw XML, so it sails straight through [`xml_nesting_too_deep`]
//! untouched — yet `usvg` resolves each `<use>` by *recursively cloning* the referenced content, so
//! resolution stack depth scales with chain length, and a long enough chain overflows a small thread stack
//! exactly like deep literal nesting does. A prior investigation (CPE-1414, still parked/deferred — see
//! `Ticketing/Tickets/Deferred/CPE-1414_svg-use-reference-cycle-dos.md`) attempted a *cycle-only* guard for
//! a related `<use>`/`<symbol>` mutual-reference DoS and burned three attempts, each defeated by an
//! adversarial reviewer finding a small-stack-overflow bypass in the hand-rolled href-resolution: an
//! entity-encoded `href` (`&#35;b`), an internal-subset DTD entity (`<!ENTITY r "#b">`), and — most
//! subtly — checking plain `href` before `xlink:href` when `usvg`'s own `resolve_href` checks
//! `xlink:href` FIRST. None of those three attempts ever actually landed in this file (CPE-1414 is still
//! Deferred with no shipped code), so [`use_reference_chain_too_deep`] below is a fresh, non-recursive
//! reference-graph walk built from scratch, but it deliberately reuses CPE-1414's hard-won, adversarially-
//! validated approach to href resolution — SVGZ-decompress + parse with `usvg`'s own exact
//! `roxmltree::ParsingOptions{allow_dtd: true, ..}`, then resolve `xlink:href` before `href` — so it can't
//! be defeated by any of those three same bypass shapes. Because the walk tracks each node's longest
//! reference-chain depth in the same DFS pass that also detects a revisited (`InProgress`) node as a
//! cycle, it incidentally also closes CPE-1414's mutual-`<symbol>` cycle finding (see the doc comment on
//! [`use_reference_chain_too_deep`] and the now-un-`#[ignore]`d regression test in
//! `tests/thumb_svg_panic_safety.rs`) — CPE-1414 itself is left Deferred/untouched as a ticket (out of
//! this ticket's scope to close), but the underlying crash it reported no longer reproduces.
//!
//! **CPE-1437 attempt 2 — depth-prediction retired as the safety bar, a guaranteed-large stack adopted
//! instead:** an independent adversarial security audit of attempt 1 (above) found a third bypass, this
//! time a *composition* one: `usvg`'s real native recursion cost is [`<use>`-hop count] × [each hop's
//! target's own internal `<g>`-nesting depth] — not either dimension alone. [`use_reference_chain_too_deep`]
//! only counted hops (treating a `<use>` pointing at a deeply-`<g>`-nested container as "1 hop"), and
//! [`xml_nesting_too_deep`] only counts a single document's own max literal nesting (which resets between
//! unrelated siblings) — so neither bounds the product, and the auditor built concrete payloads well under
//! *both* caps (≤128 hops, ≤64 nesting) that still overflowed the 256KB probe (e.g. 10 containers each
//! `<g>`-nested 20 deep, chained by 11 `<use>` hops; or a single `<use>` into one 40-deep `<g>` container).
//! The audit also surfaced a **pre-existing CPE-1413 hole**, unrelated to `<use>` at all: plain `<g>`
//! nesting with *no* `<use>` anywhere overflowed the 256KB probe around depth ~35 during `usvg`'s tree-
//! *conversion* pass (building its own scene-tree from the parsed XML DOM — a separate, heavier recursive
//! pass than the raw `roxmltree` parse CPE-1413 originally profiled), which is **under** CPE-1413's
//! existing cap of 64 — so a legitimately-shaped, cap-passing document could already overflow the probe
//! before CPE-1437 was ever touched.
//!
//! Two independent depth-prediction guards (CPE-1414's three cycle-guard attempts, then this file's own
//! CPE-1437 attempt 1) have now each been defeated by an adversarial reviewer finding a dimension of
//! `usvg`'s real recursion cost the guard's model didn't account for. Extending either guard with a third
//! dimension (nesting × hops, or worse) only invites a fourth bypass — modeling a large, evolving C-like
//! library's exact internal recursion shape from the outside is inherently brittle. So attempt 2 adopts
//! the ticket's own alternative fix direction as the **primary, durable** guarantee instead:
//! [`rasterize_svg`] now does the actual `usvg` parse/convert/`resvg` render on a dedicated thread with a
//! stack ([`RASTERIZE_STACK_SIZE`], 16MiB) sized to comfortably outlast `usvg`'s own hard recursion cap
//! (1024 levels) under any plausible per-level stack cost — see [`rasterize_svg_on_a_guaranteed_stack`].
//! Since `usvg` itself always enforces that 1024-level cap (returning its own `Err` past it) regardless of
//! how the depth got distributed across nesting/hops/composition, giving it enough real stack to always
//! *reach* that cap gracefully closes every variant of this bug class at once, present and future, without
//! predicting anything about the input shape. [`xml_nesting_too_deep`] and [`use_reference_chain_too_deep`]
//! are kept as cheap fast-reject checks (genuinely pathological input still shouldn't pay for a 16MiB
//! thread spawn and a near-1024-level `usvg` walk) but are explicitly no longer relied on as the
//! stack-overflow safety bar — see [`rasterize_svg`]'s doc comment.
//!
//! **A subtlety this fix's own verification caught:** [`use_reference_chain_too_deep`] is *not* the same
//! kind of "cheap, provably non-recursive" check [`xml_nesting_too_deep`] is — to mirror `usvg`'s exact
//! entity/DTD decoding (see that function's own doc comment) it calls the REAL, recursive
//! `roxmltree::Document::parse_with_options`, the identical parser class CPE-1413 originally found recurses
//! per XML nesting level with no cap of its own. An early version of this attempt left that call on the
//! *caller's* thread (reasoning it ran "before" the big-stack spawn, so it'd only ever see shallow input
//! that already passed `xml_nesting_too_deep`'s 64-level cap) — but a small-stack diagnostic during
//! verification showed a BARE `roxmltree` parse of a document only ~42 levels deep (comfortably under that
//! 64-level cap) already overflows a 256KB stack by itself. So `use_reference_chain_too_deep` now runs
//! *inside* [`rasterize_svg_on_a_guaranteed_stack`]'s closure, on the same guaranteed stack as the render —
//! only [`xml_nesting_too_deep`]'s genuinely flat byte-scan loop is safe to run on the caller's own thread.

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

/// `roxmltree`'s two lifetime parameters (`'a`: the borrow of the parsed `Document`/`Node`s, `'input`:
/// the borrow of the source text) always coincide in this module's own usage below — the guard parses
/// `text` and walks the resulting tree entirely within one function, never separately reborrowing either
/// — so this alias collapses them to a single lifetime purely for readability.
type XmlNode<'a> = resvg::usvg::roxmltree::Node<'a, 'a>;

/// Namespace URI for `xlink:href`. Matters because `usvg`'s own attribute resolution (`resolve_href` in
/// usvg-0.45.1's `parser/svgtree/parse.rs`) checks the **`xlink:href`-namespaced attribute FIRST**, and
/// only falls back to the un-namespaced `href` if that one is absent. This exact precedence is one of the
/// three bypass classes the CPE-1414 investigation found in earlier (never-shipped) guard attempts: a
/// guard that checked `href` before `xlink:href`, or matched by local attribute name only, could disagree
/// with `usvg` about which target an element with *both* attributes present actually resolves to (e.g.
/// `<use href="#leaf" xlink:href="#other"/>` — the guard would see `#leaf`, `usvg` resolves `#other`).
/// [`resolve_use_href`] reuses this exact order to close that class here too.
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// The deepest `<use>` **reference chain** ([`use_reference_chain_too_deep`]'s walk: `<use>` A -> target
/// B -> (if B is itself a `<use>`, or contains one) target C -> ...) allowed before `rasterize_svg`
/// refuses the document outright (CPE-1437). `usvg` resolves each hop by *recursively cloning* the
/// referenced content, so resolved-tree recursion depth scales with chain length; unlike
/// [`MAX_SVG_NESTING_DEPTH`] (literal XML nesting), a flat sibling chain of `<use>` elements each
/// referencing the previous is only ~1 level deep in the raw XML, so it passes that guard untouched while
/// still driving `usvg` — and, on this codebase's 256KB small-stack test probe, the whole process — to the
/// same recursion depth as if it *were* nested that deep.
///
/// 128 is sized the same way [`MAX_SVG_NESTING_DEPTH`] (64) was: a real hand-authored or tool-exported
/// SVG's `<use>` indirection is essentially always 1-3 hops (icon-sprite sheets referencing a single base
/// shape are the deepest realistic case, rarely past single digits), so 128 costs nothing for legitimate
/// artwork; it's comfortably below the ~500-hop chain CPE-1437 confirmed reliably `STATUS_STACK_OVERFLOW`s
/// a 256KB debug-build thread stack (the same wide margin [`MAX_SVG_NESTING_DEPTH`]'s 64 keeps under its
/// own ~500-level empirical crash depth); and it's comfortably below `usvg`'s own internal recursion cap
/// of 1024, which is sized for `usvg`'s *own* DoS protection on a normal-sized stack, not for this
/// codebase's small-stack safety bar.
const MAX_USE_CHAIN_DEPTH: usize = 128;

/// Mirrors `svgtypes::IRI::from_str`'s exact fragment grammar by hand (`usvg`'s `resolve_href` parses a
/// resolved `href`/`xlink:href` value with `svgtypes::IRI::from_str`, and the CPE-1414 investigation
/// found that any hand-rolled stand-in that doesn't match this precisely is an evasion vector — e.g.
/// silently accepting or guessing at a malformed value `usvg` would actually reject). `svgtypes` is only a
/// *transitive* dependency here (pulled in by `usvg` internally; not re-exported), and this crate's
/// no-new-dependencies guardrail rules out adding it just to call one ~20-line parser, so this
/// reimplements the grammar byte-for-byte instead (see svgtypes 0.15.3's `funciri.rs`
/// `IRI::from_str`/`Stream::parse_iri`, the reference this mirrors): skip leading XML whitespace
/// (space/tab/CR/LF), require a literal `#`, take bytes up to (not including) the first literal space
/// (`0x20`) byte or end-of-string as the fragment id (must be non-empty), then allow only further XML
/// whitespace before the end. Anything else — including a value with no leading `#` at all, e.g. a full
/// external URL — is a parse failure and returns `None`, exactly like `IRI::from_str`, never guessing a
/// partial id out of a bypass-shaped value.
fn parse_iri_fragment(text: &str) -> Option<&str> {
    fn is_xml_space(b: u8) -> bool {
        matches!(b, b' ' | b'\t' | b'\n' | b'\r')
    }
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && is_xml_space(bytes[i]) {
        i += 1;
    }
    if bytes.get(i) != Some(&b'#') {
        return None;
    }
    i += 1;
    let start = i;
    while i < bytes.len() && bytes[i] != b' ' {
        i += 1;
    }
    if i == start {
        return None; // empty fragment, e.g. a bare "#"
    }
    let id_end = i;
    while i < bytes.len() && is_xml_space(bytes[i]) {
        i += 1;
    }
    if i != bytes.len() {
        return None; // trailing non-space garbage after the fragment
    }
    // These byte offsets only ever land on ASCII delimiters ('#', ' ', or XML whitespace), which are
    // always valid UTF-8 char boundaries, so this slice can't panic on a multi-byte boundary.
    Some(&text[start..id_end])
}

/// Resolves a node's link-target fragment id exactly the way `usvg`'s own `resolve_href` does:
/// `xlink:href` FIRST, falling back to plain `href` only if that's absent (see [`XLINK_NS`]'s doc comment
/// for why this precedence specifically matters), then parses the value with [`parse_iri_fragment`]
/// (mirroring `svgtypes::IRI::from_str`).
fn resolve_use_href<'a>(node: XmlNode<'a>) -> Option<&'a str> {
    let link_value = node.attribute((XLINK_NS, "href")).or_else(|| node.attribute("href"))?;
    parse_iri_fragment(link_value)
}

/// The `<use>` elements a resolution of `target` will encounter next. If `target` is itself a `<use>`
/// element, resolving it continues directly to *its own* target — a single further hop, the flat-chain
/// shape CPE-1437 reported. Otherwise `target` is a container (`<symbol>`, `<g>`, plain shape, ...), and
/// rendering its content walks into every `<use>` anywhere in its subtree (at any nesting depth) — each of
/// those is a further hop too. This dual rule is also what lets the same walk catch the CPE-1414 mutual-
/// `<symbol>`-cycle shape (`<symbol id="a">` containing a `<use>` to `#b`, `<symbol id="b">` containing a
/// `<use>` back to `#a`): the edge from the `id="a"` target to the `<use href="#b">` nested inside it is
/// exactly the kind of hop this returns, so the walk sees `a -> b -> a` as a real cycle even though
/// neither `<use>` element directly names the other.
fn use_targets_reached_via(target: XmlNode<'_>) -> Vec<XmlNode<'_>> {
    if target.has_tag_name("use") {
        vec![target]
    } else {
        target.descendants().filter(|n| n.has_tag_name("use")).collect()
    }
}

/// The `<use>` hops directly reachable from resolving `use_node`'s own href (see
/// [`use_targets_reached_via`]). A direct self-reference (`use_node`'s target resolves back to itself) is
/// treated as *no* hop at all, matching `usvg`'s own explicit `link == node` self-reference guard (it
/// silently skips rendering rather than recursing) — reusing that same real-world-harmless special case
/// here avoids over-rejecting the (contrived but legal, and already covered by its own small-stack
/// regression test below) `<use href="#self">` idiom as if it were a reference cycle.
fn use_chain_edges<'a>(use_node: XmlNode<'a>, id_map: &HashMap<&'a str, XmlNode<'a>>) -> Vec<XmlNode<'a>> {
    let Some(frag) = resolve_use_href(use_node) else {
        return Vec::new();
    };
    let Some(target) = id_map.get(frag).copied() else {
        return Vec::new();
    };
    if target == use_node {
        return Vec::new();
    }
    use_targets_reached_via(target)
}

/// Cheap(ish), non-recursive guard against a maliciously (or accidentally) deep `<use>` reference chain
/// (CPE-1437), run before the document is handed to `usvg`/`resvg::render`. See the module doc comment for
/// the full backstory; in short this complements [`xml_nesting_too_deep`] (CPE-1413), which only bounds
/// *literal* XML nesting and is blind to a flat sibling `<use>` chain.
///
/// Mirrors `usvg`'s own preprocessing byte-for-byte before parsing — the CPE-1414 investigation's root
/// finding was that three separate prior guard attempts were each bypassed by hand-rolling this
/// preprocessing step slightly differently than `usvg` itself does: SVGZ gzip-decompresses first via
/// `usvg`'s own public `decompress_svgz` (not a reimplementation, so it can't drift), then parses with the
/// exact same `roxmltree::ParsingOptions { allow_dtd: true, .. }` that `usvg::Tree::from_str` uses — so a
/// DTD internal-subset entity (`<!ENTITY r "#b">` + `href="&r;"`) expands here exactly as `usvg` will
/// expand it, and a numeric-entity-encoded `href` (`&#35;b`) decodes to the same `#b` `usvg` sees, closing
/// both of the entity-based bypass classes CPE-1414 found. [`resolve_use_href`] then mirrors `usvg`'s
/// exact `xlink:href`-before-`href` attribute precedence (the third bypass class), so this walk can't be
/// fooled by any of the three shapes that defeated those earlier attempts.
///
/// Deliberately does **not** pre-filter on whether the raw bytes contain a `"use"` substring before paying
/// for the full parse: that kind of fast path is itself a potential bypass (an entity/DTD-obfuscated
/// `<use>`-equivalent might not contain the literal substring pre-decode), and this is a low-frequency,
/// security-sensitive code path (thumbnail generation, not a hot per-frame loop) where correctness is
/// worth more than shaving a parse.
///
/// Walks the reference graph with an explicit heap-allocated stack (never the real call stack, so this
/// scan itself can't overflow no matter how deep or cyclic the input is), doing an iterative post-order DFS
/// from every `<use>` element in the document, memoizing each node's resolved chain depth so no node is
/// ever walked twice. A **cycle** (revisiting a node that's still `InProgress` on the current path — this
/// is what also catches the CPE-1414 mutual-`<symbol>` reference cycle, since that's an unbounded chain by
/// construction) is treated as exceeding the cap immediately: either way `usvg` would recurse without
/// bound (a true cycle) or well past this codebase's small-stack safety margin (a merely very long chain),
/// so both are rejected identically here rather than trying to special-case "genuinely infinite" vs. "just
/// very deep". Also bails out the moment the *currently open* DFS path alone exceeds `max_depth` (before
/// exploring any further), so a single pathological chain can't force this scan to do more than
/// `O(max_depth)` work before rejecting it.
fn use_reference_chain_too_deep(bytes: &[u8], max_depth: usize) -> bool {
    let decompressed;
    let text: &str = if bytes.starts_with(&[0x1f, 0x8b]) {
        decompressed = match resvg::usvg::decompress_svgz(bytes) {
            Ok(d) => d,
            Err(_) => return false, // malformed gzip: usvg's real parse will report this itself
        };
        match std::str::from_utf8(&decompressed) {
            Ok(s) => s,
            Err(_) => return false,
        }
    } else {
        match std::str::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => return false,
        }
    };

    let xml_opt = resvg::usvg::roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let doc = match resvg::usvg::roxmltree::Document::parse_with_options(text, xml_opt) {
        Ok(d) => d,
        Err(_) => return false, // malformed XML: usvg's real parse will report this itself
    };

    // id -> first element with that id, mirroring usvg's own id_map build exactly (first occurrence in
    // document order wins; `HashMap::entry(..).or_insert(..)` only inserts when the key is absent).
    let mut id_map: HashMap<&str, XmlNode> = HashMap::new();
    for node in doc.descendants() {
        if let Some(id) = node.attribute("id") {
            id_map.entry(id).or_insert(node);
        }
    }

    let use_nodes: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("use")).collect();
    if use_nodes.is_empty() {
        return false;
    }

    enum VisitState {
        InProgress,
        Done(usize),
    }
    let mut state: HashMap<XmlNode, VisitState> = HashMap::new();

    for &start in &use_nodes {
        if state.contains_key(&start) {
            continue; // already resolved as part of an earlier start node's walk
        }

        // Explicit heap-allocated stack, non-recursive DFS: each frame is (node, its outgoing edges,
        // index of the next edge to explore).
        let mut stack: Vec<(XmlNode, Vec<XmlNode>, usize)> = Vec::new();
        state.insert(start, VisitState::InProgress);
        stack.push((start, use_chain_edges(start, &id_map), 0));

        while !stack.is_empty() {
            let top = stack.len() - 1;
            let idx = stack[top].2;
            if idx < stack[top].1.len() {
                let next = stack[top].1[idx];
                stack[top].2 += 1;
                match state.get(&next) {
                    Some(VisitState::Done(_)) => {} // already resolved; used when this frame unwinds
                    Some(VisitState::InProgress) => return true, // cycle -> unbounded chain -> reject
                    None => {
                        state.insert(next, VisitState::InProgress);
                        let edges = use_chain_edges(next, &id_map);
                        stack.push((next, edges, 0));
                        if stack.len() > max_depth {
                            // The currently-open DFS path is already longer than the cap. Whatever depth
                            // the deepest node on it ends up with (computed on unwind below) can only be
                            // >= this path length, so this can reject right now without exploring further.
                            return true;
                        }
                    }
                }
            } else {
                // All of `node`'s edges are resolved (`Done`) — compute and memoize its own chain depth.
                let (node, edges, _) = stack.pop().unwrap();
                let max_child_depth = edges
                    .iter()
                    .map(|e| match state.get(e) {
                        Some(VisitState::Done(d)) => *d,
                        _ => 0,
                    })
                    .max()
                    .unwrap_or(0);
                let depth = 1 + max_child_depth;
                if depth > max_depth {
                    return true;
                }
                state.insert(node, VisitState::Done(depth));
            }
        }
    }

    false
}

/// The stack size [`rasterize_svg_on_a_guaranteed_stack`] gives the dedicated thread that does the real
/// `usvg` parse/convert/render work (CPE-1437 attempt 2). 16MiB, chosen the way the coordinator's fix
/// direction specified: usvg's own hard recursion cap is 1024 levels regardless of caller stack size, and
/// this codebase's own 256KB small-stack panic-safety probe empirically overflowed well under 500 levels
/// (in some shapes — see the module doc comment — under 40). Even pessimistically assuming most of that
/// 256KB is fixed thread overhead rather than available for recursion (say only ~150KB of it usable),
/// that puts the real per-level stack cost at a few KB; 16MiB is 64x that probe's total size, giving
/// several thousand levels of headroom — comfortably past usvg's own 1024 cap under any reasonable
/// per-level cost estimate. Empirically confirmed safe against every adversarial payload in
/// `tests/thumb_svg_panic_safety.rs` (composition-of-hops-and-nesting, plain deep nesting, the mutual
/// `<symbol>` cycle, and the flat 500-hop chain).
const RASTERIZE_STACK_SIZE: usize = 16 * 1024 * 1024;

/// Rasterize `bytes` (the contents of an `.svg` file) to an RGBA image whose longest edge is at most
/// `max_edge` pixels, preserving the document's aspect ratio. Never panics — and, critically, never
/// **stack-overflows the calling thread**, which is a stronger guarantee than "never panics" since a
/// stack overflow is uncatchable and aborts the whole process regardless of `catch_unwind`: see
/// [`rasterize_svg_on_a_guaranteed_stack`] for how. A malformed document, an implausible declared size, or
/// a render-target allocation failure all return `Err`.
///
/// [`xml_nesting_too_deep`] and [`use_reference_chain_too_deep`] still run first as cheap, non-recursive
/// fast-reject checks — genuinely useful defense-in-depth against obviously pathological input (an
/// absurdly deep document or an absurdly long/cyclic `<use>` chain costs real CPU/memory even with a big
/// enough stack to survive it) — but per CPE-1437 attempt 2 they are **no longer the stack-overflow safety
/// bar**. See the module doc comment for why: two separate depth-prediction guards (CPE-1414's cycle
/// guard attempts and this file's own CPE-1437 attempt 1) were each defeated by an adversarial reviewer
/// finding an input shape the guard's model of "what makes usvg recurse deeply" didn't account for, most
/// recently a payload that composes `<use>`-hop count *with* each hop's target's internal `<g>`-nesting
/// depth — neither guard alone bounds that product, and it turned out usvg's tree-*conversion* pass (as
/// opposed to the raw XML parse CPE-1413 originally profiled) has a real crash threshold on a 256KB stack
/// low enough that even literal nesting alone, comfortably under CPE-1413's existing cap of 64, can
/// overflow it. Trying to extend either guard with a third dimension only invites a fourth bypass; the
/// durable fix is to stop predicting and just guarantee enough real stack.
pub fn rasterize_svg(bytes: &[u8], max_edge: u32) -> Result<DynamicImage, String> {
    // Only the pure byte-scan guard runs on the CALLER's own thread — it's provably non-recursive (a flat
    // loop with an integer counter, see its own doc comment), so it can't overflow any stack regardless of
    // input depth, and rejecting an obviously-pathological document this cheaply avoids paying for a
    // dedicated large-stack thread spawn at all in the common "somebody sent garbage" case.
    if xml_nesting_too_deep(bytes, MAX_SVG_NESTING_DEPTH) {
        return Err("SVG element nesting too deep".to_string());
    }

    // Everything else — including `use_reference_chain_too_deep` — runs inside the guaranteed-large-stack
    // thread below. This is deliberate, not an oversight: `use_reference_chain_too_deep` itself calls the
    // REAL `roxmltree::Document::parse_with_options` (needed to mirror `usvg`'s own entity/DTD decoding —
    // see that function's doc comment), which is exactly the same per-nesting-level-recursive parser
    // `xml_nesting_too_deep` exists to keep away from a small caller stack in the first place. A CPE-1437
    // attempt 2 diagnostic (see the module doc comment) confirmed a BARE `roxmltree` parse of a document
    // only ~42 levels deep — comfortably under `xml_nesting_too_deep`'s own 64-level cap — already
    // overflows a 256KB stack by itself, so leaving that parse on the caller's thread would have silently
    // reintroduced the exact bug class this whole attempt exists to close.
    rasterize_svg_on_a_guaranteed_stack(bytes.to_vec(), max_edge)
}

/// Does the actual `<use>`-chain check *and* the `usvg` parse + convert + `resvg` render on a **dedicated
/// thread** with [`RASTERIZE_STACK_SIZE`] of stack, regardless of the calling thread's own stack size
/// (which, in production, is already the Tokio `spawn_blocking` default of 2MiB — plenty on its own, but
/// this function no longer wants to depend on that; a caller on a smaller stack, like this crate's own
/// small-stack panic-safety probe, must be just as safe). `rasterize_svg` now **owns** its stack
/// requirement instead of inheriting the caller's, so no input `usvg` itself is willing to accept — it has
/// its own hard cap of 1024 recursion levels and 1,000,000 elements — can overflow it, no matter how that
/// recursion cost is distributed across literal nesting, `<use>` hops, or (per CPE-1437 attempt 2's
/// finding) any composition of the two. This closes the composition bypass *and* the pre-existing
/// CPE-1413 conversion-depth hole at once, without needing to model `usvg`'s internals at all.
///
/// [`use_reference_chain_too_deep`] runs FIRST inside this closure (still cheap relative to a full render,
/// and still valuable as defense-in-depth against a pathologically long/cyclic `<use>` chain), but now on
/// this function's own guaranteed stack rather than the caller's — see [`rasterize_svg`]'s doc comment for
/// why that placement specifically matters.
///
/// A panic inside the closure (e.g. an allocation failure surfaced as a panic rather than an `Err`) is
/// caught via the thread's own unwind boundary and turned into a graceful `Err` through `JoinHandle::join`
/// — only a genuine stack *overflow* is uncatchable, and that's exactly the failure mode this function
/// exists to make unreachable. Failing to spawn the thread at all (OS resource exhaustion) is itself just
/// another `Err`, never a panic.
fn rasterize_svg_on_a_guaranteed_stack(bytes: Vec<u8>, max_edge: u32) -> Result<DynamicImage, String> {
    let render = move || -> Result<DynamicImage, String> {
        if use_reference_chain_too_deep(&bytes, MAX_USE_CHAIN_DEPTH) {
            return Err("SVG <use> reference chain too deep".to_string());
        }

        let opt = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_data(&bytes, &opt).map_err(|e| e.to_string())?;

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

        let mut pixmap = resvg::tiny_skia::Pixmap::new(out_w, out_h)
            .ok_or("could not allocate an SVG render target")?;
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
    };

    match std::thread::Builder::new().stack_size(RASTERIZE_STACK_SIZE).spawn(render) {
        Ok(handle) => handle
            .join()
            .unwrap_or_else(|_| Err("SVG rasterization thread panicked".to_string())),
        Err(e) => Err(format!("failed to spawn the SVG rasterization thread: {e}")),
    }
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

    // -----------------------------------------------------------------------------------------
    // CPE-1437: `<use>` reference-chain depth guard.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn iri_fragment_parses_a_plain_fragment() {
        assert_eq!(parse_iri_fragment("#id"), Some("id"));
        assert_eq!(parse_iri_fragment("   #id   "), Some("id"));
        assert_eq!(parse_iri_fragment("#1"), Some("1"));
    }

    #[test]
    fn iri_fragment_rejects_non_fragment_and_malformed_values() {
        assert_eq!(parse_iri_fragment("no-hash-here"), None, "no leading '#' at all");
        assert_eq!(parse_iri_fragment("https://example.com/x.svg#id"), None, "external URL, not a bare fragment");
        assert_eq!(parse_iri_fragment("#"), None, "empty fragment");
        assert_eq!(parse_iri_fragment("# id"), None, "space immediately after '#'");
        assert_eq!(parse_iri_fragment("#id trailing garbage"), None, "trailing non-space data");
    }

    /// An SVG with a `<use>` element having BOTH a plain `href` and an `xlink:href`, pointing at two
    /// different targets — the exact CPE-1414 bypass-3 shape (a guard that checked local-name `href`
    /// before namespaced `xlink:href`, source-order rather than namespace-priority, would read the wrong
    /// target). `resolve_use_href` must resolve to the `xlink:href` target, matching usvg's own
    /// `resolve_href` precedence exactly.
    #[test]
    fn resolve_use_href_prefers_xlink_href_over_plain_href_matching_usvg_precedence() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <rect id="leaf" width="1" height="1"/>
            <rect id="other" width="1" height="1"/>
            <use id="u" href="#leaf" xlink:href="#other"/>
        </svg>"##;
        let opt = resvg::usvg::roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
        let doc = resvg::usvg::roxmltree::Document::parse_with_options(
            std::str::from_utf8(svg).unwrap(),
            opt,
        )
        .unwrap();
        let use_node = doc.descendants().find(|n| n.has_tag_name("use")).unwrap();
        assert_eq!(resolve_use_href(use_node), Some("other"), "xlink:href must win over plain href");
    }

    /// A flat, ACYCLIC chain of `<use>` elements each referencing the previous — the CPE-1437 reproducer
    /// shape (`#u1` <- `#u2` <- ... <- `#u{n}`), triggered from a single outer `<use>` that starts the
    /// chain.
    fn flat_use_chain_svg(n: usize) -> Vec<u8> {
        let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
        s.push_str(r##"<rect id="u0" width="10" height="10" fill="#f00"/>"##);
        for i in 1..=n {
            s.push_str(&format!(r##"<use id="u{i}" href="#u{prev}"/>"##, prev = i - 1));
        }
        s.push_str("</svg>");
        s.into_bytes()
    }

    #[test]
    fn use_chain_guard_rejects_a_deep_flat_chain_and_allows_a_shallow_one() {
        // CPE-1437's exact finding: ~500 links passes both CPE-1413's nesting guard (siblings, depth ~1)
        // and would not be flagged as a reference cycle (it's acyclic) — must still be rejected as a
        // too-deep reference chain.
        assert!(use_reference_chain_too_deep(&flat_use_chain_svg(500), MAX_USE_CHAIN_DEPTH));
        // A realistic sprite-sheet-style chain (a couple of hops) must not be rejected.
        assert!(!use_reference_chain_too_deep(&flat_use_chain_svg(3), MAX_USE_CHAIN_DEPTH));
        assert!(!use_reference_chain_too_deep(&square_svg(10, 10), MAX_USE_CHAIN_DEPTH), "no <use> at all");
    }

    #[test]
    fn use_chain_guard_boundary_exactly_at_the_cap_is_allowed_one_more_is_rejected() {
        // A chain of exactly MAX_USE_CHAIN_DEPTH links resolves to chain depth == the cap, which the
        // guard's own `depth > max_depth` check must allow; one link more must tip it over.
        assert!(
            !use_reference_chain_too_deep(&flat_use_chain_svg(MAX_USE_CHAIN_DEPTH), MAX_USE_CHAIN_DEPTH),
            "a chain exactly at the cap must be allowed"
        );
        assert!(
            use_reference_chain_too_deep(&flat_use_chain_svg(MAX_USE_CHAIN_DEPTH + 1), MAX_USE_CHAIN_DEPTH),
            "one link past the cap must be rejected"
        );
    }

    #[test]
    fn use_chain_guard_rejects_the_cpe_1414_mutual_symbol_cycle() {
        // Two <symbol>s each referencing the other via a nested <use>, plus an outer trigger <use> — the
        // exact reproducer from the (still-Deferred) CPE-1414 investigation. This is acyclic in neither
        // the literal XML nesting (each <symbol> is shallow) nor a naive "does this <use> point at
        // another <use>" check (each <use> points at a <symbol>), but IS a real reference cycle once
        // <symbol>-to-nested-<use> containment edges are followed — the walk must catch it.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <symbol id="a"><use xlink:href="#b"/></symbol>
            <symbol id="b"><use xlink:href="#a"/></symbol>
            <use xlink:href="#a"/>
        </svg>"##;
        assert!(use_reference_chain_too_deep(svg, MAX_USE_CHAIN_DEPTH));
    }

    #[test]
    fn use_chain_guard_allows_a_direct_self_reference() {
        // `<use href="#self">` — usvg treats this as a harmless no-op (link == node), not a cycle; the
        // guard must not over-reject this common-enough idiom.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <use id="self" href="#self"/>
        </svg>"##;
        assert!(!use_reference_chain_too_deep(svg, MAX_USE_CHAIN_DEPTH));
    }

    #[test]
    fn use_chain_guard_resolves_a_numeric_entity_encoded_href_matching_usvg_decoding() {
        // CPE-1414 bypass class 1: a hand-rolled byte-scan guard that doesn't decode XML character
        // references would read the raw text "&#35;u0" (no leading literal '#') and see no edge at all,
        // while usvg/roxmltree decode it to "#u0" and DO resolve it — a guard that silently under-counts
        // here is exactly the kind of guard CPE-1414's three attempts kept falling to. Since this guard
        // parses via roxmltree (which performs the same entity decoding usvg relies on), it must see this
        // edge too: confirmed here by showing a cap of 0 (which any real resolved edge must exceed) DOES
        // flag it, proving the edge wasn't silently dropped.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <rect id="u0" width="10" height="10" fill="#f00"/>
            <use id="u1" xlink:href="&#35;u0"/>
        </svg>"##;
        assert!(!use_reference_chain_too_deep(svg, MAX_USE_CHAIN_DEPTH), "a single resolved hop is well under the real cap");
        assert!(
            use_reference_chain_too_deep(svg, 0),
            "entity-encoded href must resolve to a real edge, not be silently dropped"
        );
    }

    #[test]
    fn use_chain_guard_resolves_dtd_entity_hrefs_into_a_real_chain_matching_usvg_decoding() {
        // CPE-1414 bypass class 2: same idea as the numeric-entity case above, but for an internal-subset
        // DTD entity (`<!ENTITY rN "#u...">` + `href="&rN;"`) — a guard parsed WITHOUT `allow_dtd: true`
        // (or one that doesn't expand entities in attribute values at all) would see these hrefs as inert
        // and never build the chain. Build a real N-hop chain entirely out of per-link DTD entities and
        // confirm it's caught as too deep, exactly like the literal `flat_use_chain_svg` case above.
        let n = 200;
        let mut doctype = String::from("<!DOCTYPE svg [");
        for i in 1..=n {
            doctype.push_str(&format!("<!ENTITY r{i} \"#u{prev}\">", prev = i - 1));
        }
        doctype.push_str("]>");
        let mut s = doctype;
        s.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
        s.push_str(r##"<rect id="u0" width="10" height="10" fill="#f00"/>"##);
        for i in 1..=n {
            s.push_str(&format!(r##"<use id="u{i}" href="&r{i};"/>"##));
        }
        s.push_str("</svg>");
        assert!(
            use_reference_chain_too_deep(s.as_bytes(), MAX_USE_CHAIN_DEPTH),
            "a DTD-entity-built chain must resolve its edges and be caught as too deep, just like a literal one"
        );
    }

    #[test]
    fn rasterize_svg_rejects_a_deep_flat_use_chain_gracefully() {
        let err = rasterize_svg(&flat_use_chain_svg(500), 32);
        assert!(err.is_err(), "an implausibly deep <use> reference chain must be rejected, not risk a stack overflow");
    }

    #[test]
    fn rasterize_svg_renders_a_legitimate_svg_with_a_couple_of_uses_fine() {
        // A couple of <use>s referencing a single base shape — the realistic "icon sprite sheet" shape —
        // must still render normally; the chain-depth cap must not over-reject legitimate artwork.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
            <rect id="base" width="10" height="10" fill="#ff0000"/>
            <use href="#base" x="0"/>
            <use href="#base" x="10"/>
        </svg>"##;
        let img = rasterize_svg(svg, 32).unwrap();
        assert!(img.width() > 0 && img.height() > 0);
    }
}
