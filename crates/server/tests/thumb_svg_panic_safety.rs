//! Adversarial panic/DoS-safety battery for SVG thumbnail rendering (CPE-1413, epic CPE-1002 "File
//! inspection & safety utilities").
//!
//! `crates/server/src/thumb_svg.rs`'s [`rasterize_svg`] hands untrusted `.svg` file bytes to
//! `usvg::Tree::from_data`, which parses them into a DOM via `roxmltree::Document::parse` — the EXACT
//! same XML-tree-consumer shape as the WebDAV `PROPFIND`-response stack-overflow DoS fixed in CPE-1398
//! (`crates/webdav/src/lib.rs`'s `parse_multistatus`/`xml_nesting_too_deep`). This file is the SVG
//! analogue of that investigation: a generic hostile-bytes battery (reusing `tests/common/mod.rs`'s
//! shared battery + `catch_unwind` harness, same as `parser_panic_safety.rs` and
//! `binary_data_preview_panic_safety.rs`), plus SVG-specific cases the generic battery can't reach —
//! huge flat element counts, adversarial attribute values, and, most importantly, deeply-nested
//! `<g>`/`<svg>` elements and `<use>` reference cycles, both run on a small (`256KB`) thread stack per the
//! ticket's brief, mirroring exactly how CPE-1398's re-review empirically reproduced its webdav bypass on
//! a small stack. A stack overflow is **uncatchable** by `catch_unwind` — it aborts the whole process —
//! so "did it crash" is judged by whether the dedicated thread's `.join()` succeeds, not by any panic
//! payload.
//!
//! ## Findings (see the PR description for the full writeup)
//!
//! - **Deep `<g>` nesting was a real, confirmed stack-overflow DoS** (crashed a 256KB-stack thread well
//!   under 500 levels of `<g>` nesting, in a debug build) — `roxmltree`'s raw-XML parse recurses per
//!   nesting level with no depth cap of its own, running *before* usvg's own element-count/`use`-depth
//!   caps ever get a chance to apply. **Fixed** (CPE-1413): `rasterize_svg` now runs the same style of
//!   non-recursive pre-scan depth guard CPE-1398 used for webdav
//!   (`thumb_svg::xml_nesting_too_deep`/`MAX_SVG_NESTING_DEPTH`), rejecting implausibly deep nesting
//!   before the bytes ever reach `usvg`/`roxmltree`. [`rasterize_svg_never_stack_overflows_on_deep_nesting_on_a_small_stack`]
//!   below is the regression test for the fix.
//! - **A mutual `<use>` reference cycle (two `<symbol>`s referencing each other) was ALSO a real,
//!   confirmed stack-overflow DoS on a small stack** — usvg only special-cases *direct* self-reference and
//!   one-hop parent/sibling back-references; anything requiring 2+ hops of `xlink:href` indirection falls
//!   through to usvg's own `depth > 1024` recursion cap, and that recursion's *own* per-level stack cost is
//!   high enough to overflow a 256KB thread stack well before reaching 1024. Left unfixed for a while as a
//!   documented, reproducible, `#[ignore]`d finding (tracked as CPE-1414, still Deferred as its own
//!   ticket) rather than a rushed hand-rolled fix, per the ticket's "report it, don't force a risky fix"
//!   escalation path. **Now incidentally fixed** by CPE-1437's `<use>`-reference-chain-depth guard
//!   (`thumb_svg::use_reference_chain_too_deep`): that guard's non-recursive DFS treats a revisited node
//!   still `InProgress` on the walk as a cycle (an unbounded chain by construction) and rejects it exactly
//!   like a too-deep chain, so this reproducer is un-`#[ignore]`d below
//!   ([`rasterize_svg_use_mutual_reference_cycle_is_now_rejected_gracefully`]) and now asserts a graceful
//!   `Err`, not just "didn't crash". CPE-1414 itself remains open as a ticket (its own scope — a *pure*
//!   cycle guard — was never separately shipped as code), but the crash it originally reported no longer
//!   reproduces.
//! - **A flat, ACYCLIC chain of `<use>` elements each referencing the previous (`#u1` <- `#u2` <- ... <-
//!   `#u500`) is ALSO a real, confirmed stack-overflow DoS on a small stack** (CPE-1437 attempt 1) — it
//!   passes BOTH the deep-nesting guard above (siblings are only ~1 deep in the raw XML) and would not be
//!   flagged by a cycle-only guard (it's genuinely acyclic), yet `usvg` still resolves it via one recursive
//!   clone per link, so resolution depth scales with chain length. `thumb_svg::use_reference_chain_too_deep`
//!   bounds this reference-chain depth with its own non-recursive graph walk, run before `usvg` ever sees
//!   the bytes. [`rasterize_svg_never_stack_overflows_on_a_deep_acyclic_use_chain_on_a_small_stack`] below
//!   is the regression test.
//! - **CPE-1437 attempt 2 — an independent adversarial audit found a THIRD bypass, a composition one**:
//!   `usvg`'s real native recursion cost is `[<use>-hop count] x [each hop's target's own internal
//!   <g>-nesting depth]`, and neither `xml_nesting_too_deep` nor `use_reference_chain_too_deep` alone
//!   bounds that product — a payload well under BOTH caps individually (e.g. 10 containers each `<g>`-
//!   nested 20 deep, chained by 11 `<use>` hops; or a single `<use>` into one 40-deep `<g>` container)
//!   still overflowed the 256KB probe. The same audit also surfaced a **pre-existing CPE-1413 hole**: plain
//!   `<g>` nesting with no `<use>` at all overflowed the probe around depth ~35, during `usvg`'s tree-
//!   *conversion* pass — a separate, heavier recursive pass than the raw XML parse CPE-1413's nesting guard
//!   was originally profiled against — which is UNDER that guard's existing cap of 64. Two independent
//!   depth-prediction guards (CPE-1414's attempts, then CPE-1437 attempt 1) had now each been defeated by a
//!   dimension their model of "what makes usvg recurse" didn't cover, so attempt 2 **retires
//!   depth-prediction as the safety bar entirely**: `rasterize_svg` now does its actual `usvg`
//!   parse/convert/render on a dedicated thread with a 16MiB stack
//!   (`thumb_svg::RASTERIZE_STACK_SIZE`/`rasterize_svg_on_a_guaranteed_stack`), sized to comfortably outlast
//!   `usvg`'s own hard 1024-level recursion cap regardless of how that depth is distributed across nesting,
//!   `<use>` hops, or any composition of the two — closing this bypass class *and* the CPE-1413 hole at
//!   once without modeling `usvg`'s internals at all. The existing guards are kept as cheap fast-reject
//!   checks (see [`rasterize_svg_use_mutual_reference_cycle_is_now_rejected_gracefully`] and
//!   [`rasterize_svg_never_stack_overflows_on_a_deep_acyclic_use_chain_on_a_small_stack`], both still
//!   rejected via the fast pre-scan) but are no longer relied on as the stack-overflow bar — the new
//!   [`rasterize_svg_never_stack_overflows_on_a_composed_chain_of_nested_use_containers_on_a_small_stack`],
//!   [`rasterize_svg_never_stack_overflows_on_a_single_deeply_nested_use_target_on_a_small_stack`], and
//!   [`rasterize_svg_never_stack_overflows_on_plain_nesting_under_the_cap_on_a_small_stack`] below are the
//!   regression tests for the auditor's exact payloads, all of which now render successfully (`Ok`) rather
//!   than merely failing gracefully, since they're legitimately within `usvg`'s own bounds.
//! - **CPE-1437 attempt 3 — a large stack does NOT close everything: `usvg` has other reference-resolution
//!   recursions bounded only by total element count (~1,000,000), not by its 1024-level `<use>` cap.** A
//!   THIRD independent adversarial audit found `clippath::convert` and `mask::convert` (in `usvg`'s own
//!   source) self-recursively call themselves when a `<clipPath clip-path="url(#…)">`/`<mask
//!   mask="url(#…)">` itself has the same attribute, with NO depth cap at all (only a 1-hop self-reference
//!   guard, the same gap CPE-1414 had for `<use>`, just never fixed for these). **Confirmed reproducers: a
//!   clipPath chain overflows even the 16MiB thread at N=8000 (Ok at N=7000); a mask chain overflows by
//!   N=5000.** The same architectural gap extends to `pattern` (content converted via the same general
//!   `converter::convert_children` every other element uses, so a pattern-filled shape inside another
//!   pattern's content recurses back in via ordinary mutual recursion, not a dedicated self-call), `marker`
//!   (same mechanism for `marker-start`/`marker-mid`/`marker-end`), and `filter` (a `<filter>` can contain a
//!   `<feImage href="#element">` referencing an arbitrary element, which can have its own
//!   `filter="url(#anotherFilter)"`, chaining indirectly through the converter). **This DoS already existed
//!   on `main` before CPE-1437 — a 2MB production stack overflows at an even shorter chain than the 256KB
//!   probe does** — CPE-1437 didn't introduce it, but closing the small-stack bar means closing it too.
//!   Since no stack size can bound an `usvg`-element-count-scaled (rather than `usvg`-1024-cap-bounded)
//!   recursion, [`thumb_svg::reference_chain_too_deep`] (generalized from `<use>`-only, renamed from
//!   `use_reference_chain_too_deep`) now walks all SIX reference types — `<use>`/`<feImage>` `href`, plus
//!   `clip-path`/`mask`/`filter`/`marker-*`/`fill`/`stroke` `url(#id)` — under one unified
//!   `MAX_REFERENCE_CHAIN_DEPTH`=128 cap (renamed from `MAX_USE_CHAIN_DEPTH`, value unchanged).
//!   [`rasterize_svg_never_stack_overflows_on_a_clippath_chain_on_a_small_stack`],
//!   [`rasterize_svg_never_stack_overflows_on_a_mask_chain_on_a_small_stack`],
//!   [`rasterize_svg_never_stack_overflows_on_a_pattern_chain_on_a_small_stack`],
//!   [`rasterize_svg_never_stack_overflows_on_a_marker_chain_on_a_small_stack`], and
//!   [`rasterize_svg_never_stack_overflows_on_a_filter_feimage_chain_on_a_small_stack`] below are the
//!   regression tests (each at N well above its empirical crash floor), alongside a legit single-level
//!   positive case for each of the five newly-bounded types.

mod common;
use common::{assert_no_panic, run_battery};

use cpe_server::thumb_svg::rasterize_svg;
use std::io::Write;

/// A minimal-but-real SVG document — used as the shared battery's "magic" so `truncated_*`/
/// `magic_then_*` cases walk `rasterize_svg` into its actual XML-parsing/rendering logic rather than
/// bailing at an empty/near-empty document every time.
fn minimal_svg() -> Vec<u8> {
    br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#f00"/></svg>"##.to_vec()
}

/// An SVG whose `<g>` nesting is `depth` levels deep — the stack-overflow probe.
fn deeply_nested_svg(depth: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    s.push_str(&"<g>".repeat(depth));
    s.push_str(r##"<rect width="10" height="10" fill="#f00"/>"##);
    s.push_str(&"</g>".repeat(depth));
    s.push_str("</svg>");
    s.into_bytes()
}

/// A flat, ACYCLIC chain of `n` `<use>` elements each referencing the previous (`#u1` <- `#u2` <- ... <-
/// `#u{n}`) — the CPE-1437 stack-overflow probe. Only ~1 level deep in the raw XML (all siblings), so it
/// passes the deep-nesting guard above untouched; `usvg` still resolves each hop by recursively cloning
/// the referenced content, so resolution depth scales with `n`.
fn flat_use_chain_svg(n: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    s.push_str(r##"<rect id="u0" width="10" height="10" fill="#f00"/>"##);
    for i in 1..=n {
        s.push_str(&format!(r##"<use id="u{i}" href="#u{prev}"/>"##, prev = i - 1));
    }
    s.push_str("</svg>");
    s.into_bytes()
}

/// The CPE-1437-attempt-2 auditor's "composition" probe: `n_containers` containers, each internally
/// `<g>`-nested `nesting_per` levels deep, chained by `<use>` — container `i`'s sole content is a `<use>`
/// referencing container `i-1` (wrapped in `nesting_per` levels of `<g>`), except container 0, which just
/// wraps a plain shape. A final outer `<use>` references the last container. Each container's own literal
/// nesting individually passes `xml_nesting_too_deep` (well under `MAX_SVG_NESTING_DEPTH`=64), and the
/// chain-hop count individually passes `use_reference_chain_too_deep` (well under `MAX_USE_CHAIN_DEPTH`=128)
/// — but `usvg`'s real native recursion cost is roughly their PRODUCT, and neither guard alone bounds that.
/// `composed_use_chain_with_nested_containers_svg(10, 20)` and `(1, 40)` are the two concrete reproducers
/// the adversarial audit confirmed overflow a 256KB stack under CPE-1437 attempt 1.
fn composed_use_chain_with_nested_containers_svg(n_containers: usize, nesting_per: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    // Container 0: a plain shape, wrapped `nesting_per` <g> deep — the base of the chain, no <use> inside.
    s.push_str(r#"<g id="c0">"#);
    s.push_str(&"<g>".repeat(nesting_per));
    s.push_str(r##"<rect width="10" height="10" fill="#f00"/>"##);
    s.push_str(&"</g>".repeat(nesting_per));
    s.push_str("</g>");
    for i in 1..n_containers {
        s.push_str(&format!(r#"<g id="c{i}">"#));
        s.push_str(&"<g>".repeat(nesting_per));
        s.push_str(&format!(r##"<use href="#c{prev}"/>"##, prev = i - 1));
        s.push_str(&"</g>".repeat(nesting_per));
        s.push_str("</g>");
    }
    // Outer trigger: references the last container, starting the whole resolution chain.
    s.push_str(&format!(r##"<use href="#c{last}"/>"##, last = n_containers - 1));
    s.push_str("</svg>");
    s.into_bytes()
}

/// A flat, ACYCLIC chain of `n` `<clipPath>` elements each `clip-path`-ing the previous — the CPE-1437
/// attempt-3 auditor's `clippath::convert` self-recursion reproducer. `usvg`'s `clippath::convert` (in
/// `parser/clippath.rs`) recurses `convert(link, ...)` when a `<clipPath>` itself has a `clip-path`, with
/// NO depth cap (only a direct-self-reference guard) — confirmed to overflow even a 16MiB stack at N=8000
/// (Ok at N=7000).
fn clip_path_chain_svg(n: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    s.push_str(r##"<clipPath id="cp0"><rect width="10" height="10"/></clipPath>"##);
    for i in 1..=n {
        s.push_str(&format!(
            r##"<clipPath id="cp{i}" clip-path="url(#cp{prev})"><rect width="10" height="10"/></clipPath>"##,
            prev = i - 1
        ));
    }
    s.push_str(&format!(r##"<rect width="10" height="10" clip-path="url(#cp{n})"/>"##));
    s.push_str("</svg>");
    s.into_bytes()
}

/// The `<mask>` analogue of [`clip_path_chain_svg`] — `mask::convert` has the identical self-recursion
/// shape for `<mask mask="url(#…)">`. Confirmed to overflow even a 16MiB stack by N=5000.
fn mask_chain_svg(n: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    s.push_str(r##"<mask id="m0"><rect width="10" height="10" fill="#fff"/></mask>"##);
    for i in 1..=n {
        s.push_str(&format!(
            r##"<mask id="m{i}" mask="url(#m{prev})"><rect width="10" height="10" fill="#fff"/></mask>"##,
            prev = i - 1
        ));
    }
    s.push_str(&format!(r##"<rect width="10" height="10" mask="url(#m{n})"/>"##));
    s.push_str("</svg>");
    s.into_bytes()
}

/// A flat, ACYCLIC chain of `n` `<pattern>` elements, each filling its content shape with the previous
/// pattern via `fill="url(#…)"` — `paint_server::convert_pattern` converts a pattern's content through the
/// SAME general `converter::convert_children` every other element's children go through, so a
/// pattern-filled shape inside another pattern's content recurses back into `convert_pattern` via ordinary
/// converter mutual recursion, with no depth cap either.
fn pattern_chain_svg(n: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    s.push_str(r##"<pattern id="p0" width="1" height="1"><rect width="1" height="1" fill="#f00"/></pattern>"##);
    for i in 1..=n {
        s.push_str(&format!(
            r##"<pattern id="p{i}" width="1" height="1"><rect width="1" height="1" fill="url(#p{prev})"/></pattern>"##,
            prev = i - 1
        ));
    }
    s.push_str(&format!(r##"<rect width="10" height="10" fill="url(#p{n})"/>"##));
    s.push_str("</svg>");
    s.into_bytes()
}

/// A flat, ACYCLIC chain of `n` `<marker>` elements, each drawing a path inside its content that itself
/// `marker-start`s the previous marker — `marker::convert` converts a marker's content through the same
/// general `converter::convert_children` as pattern content, with the same missing depth cap.
fn marker_chain_svg(n: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    s.push_str(r##"<marker id="mk0" markerWidth="2" markerHeight="2"><rect width="1" height="1"/></marker>"##);
    for i in 1..=n {
        s.push_str(&format!(
            r##"<marker id="mk{i}" markerWidth="2" markerHeight="2"><path d="M0,0 L1,1" marker-start="url(#mk{prev})"/></marker>"##,
            prev = i - 1
        ));
    }
    s.push_str(&format!(r##"<path d="M0,0 L10,10" marker-start="url(#mk{n})"/>"##));
    s.push_str("</svg>");
    s.into_bytes()
}

/// A flat, ACYCLIC chain of `n` alternating `<filter>`/`<g>` pairs: each `<filter fK>` contains a
/// `<feImage href="#g{K-1}">` (referencing an ARBITRARY element, per SVG's `feImage`), and each `<g gK
/// filter="url(#fK)">` wraps a shape. `filter::convert` doesn't chain filter-to-filter directly, but
/// `feImage`'s `href` resolves via `converter::convert_element` — the same general entry point used
/// everywhere — so an element referenced by `feImage` that itself has `filter=` recurses back through the
/// converter into `filter::convert` again, indirectly chaining just as deep.
fn filter_feimage_chain_svg(n: usize) -> Vec<u8> {
    let mut s = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">"#,
    );
    s.push_str(r##"<filter id="f0"><feFlood flood-color="#f00"/></filter>"##);
    s.push_str(r##"<g id="g0" filter="url(#f0)"><rect width="1" height="1" fill="#f00"/></g>"##);
    for i in 1..=n {
        s.push_str(&format!(r##"<filter id="f{i}"><feImage href="#g{prev}"/></filter>"##, prev = i - 1));
        s.push_str(&format!(
            r##"<g id="g{i}" filter="url(#f{i})"><rect width="1" height="1" fill="#f00"/></g>"##
        ));
    }
    s.push_str(&format!(r##"<rect width="10" height="10" filter="url(#f{n})"/>"##));
    s.push_str("</svg>");
    s.into_bytes()
}

/// CPE-1444's MULTIPLICATIVE (hops × per-hop nesting) probe for `<mask>`: a chain of `hops` masks where
/// mask `m{i}`'s `mask="url(#m{i-1})"` reference sits at the BOTTOM of `nest` nested `<g>` levels, triggered
/// by an outer `<rect mask="url(#m{hops})">`. The reference-bearing inner shape is SELF-CLOSING, so total
/// literal nesting is `svg`+`mask`+`nest` — exactly 64 at `nest`=62, which PASSES `xml_nesting_too_deep`'s
/// cap — and the hop count `hops` passes `MAX_REFERENCE_CHAIN_DEPTH`=128 for `hops`≤128, yet `usvg` descends
/// each hop's `nest` `<g>` levels ON-STACK during mask resolution, so its real recursion cost is ≈ `hops ×
/// nest`. `mask_nested(127, 62)` (≈7874 frames) overflows even the 16MiB render thread on release, and the
/// debug floor is lower still (≈127×35 ≈ 4500) — the exact vector CPE-1444's combined-cost cap closes.
fn mask_nested(hops: usize, nest: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    s.push_str(r##"<mask id="m0"><rect width="10" height="10" fill="#fff"/></mask>"##);
    for i in 1..=hops {
        s.push_str(&format!(r#"<mask id="m{i}">"#));
        s.push_str(&"<g>".repeat(nest));
        s.push_str(&format!(
            r##"<rect width="10" height="10" fill="#fff" mask="url(#m{prev})"/>"##,
            prev = i - 1
        ));
        s.push_str(&"</g>".repeat(nest));
        s.push_str("</mask>");
    }
    s.push_str(&format!(r##"<rect width="10" height="10" mask="url(#m{hops})"/>"##));
    s.push_str("</svg>");
    s.into_bytes()
}

/// The `<pattern>` analogue of [`mask_nested`] — each hop's `fill="url(#p{i-1})"` (a pattern reference,
/// multiplicative) sits at the bottom of `nest` nested `<g>` levels. `pattern` content converts through the
/// same general converter that descends nesting on-stack, so it has the identical `hops × nest` cost.
fn pattern_nested(hops: usize, nest: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    s.push_str(r##"<pattern id="p0" width="1" height="1"><rect width="1" height="1" fill="#f00"/></pattern>"##);
    for i in 1..=hops {
        s.push_str(&format!(r#"<pattern id="p{i}" width="1" height="1">"#));
        s.push_str(&"<g>".repeat(nest));
        s.push_str(&format!(
            r##"<rect width="1" height="1" fill="url(#p{prev})"/>"##,
            prev = i - 1
        ));
        s.push_str(&"</g>".repeat(nest));
        s.push_str("</pattern>");
    }
    s.push_str(&format!(r##"<rect width="10" height="10" fill="url(#p{hops})"/>"##));
    s.push_str("</svg>");
    s.into_bytes()
}

/// The `<marker>` analogue of [`mask_nested`] — each hop's `marker-start="url(#mk{i-1})"` sits at the bottom
/// of `nest` nested `<g>` levels. `marker` content converts through the same general converter, same
/// `hops × nest` cost.
fn marker_nested(hops: usize, nest: usize) -> Vec<u8> {
    let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
    s.push_str(r##"<marker id="mk0" markerWidth="2" markerHeight="2"><rect width="1" height="1"/></marker>"##);
    for i in 1..=hops {
        s.push_str(&format!(r#"<marker id="mk{i}" markerWidth="2" markerHeight="2">"#));
        s.push_str(&"<g>".repeat(nest));
        s.push_str(&format!(
            r##"<path d="M0,0 L1,1" marker-start="url(#mk{prev})"/>"##,
            prev = i - 1
        ));
        s.push_str(&"</g>".repeat(nest));
        s.push_str("</marker>");
    }
    s.push_str(&format!(r##"<path d="M0,0 L10,10" marker-start="url(#mk{hops})"/>"##));
    s.push_str("</svg>");
    s.into_bytes()
}

// ---------------------------------------------------------------------------------------------
// Generic hostile-bytes battery (empty/truncated/garbage/overflowing-length-field/... — see
// `tests/common/mod.rs`), reused unmodified from `parser_panic_safety.rs`/
// `binary_data_preview_panic_safety.rs`.
// ---------------------------------------------------------------------------------------------

#[test]
fn rasterize_svg_never_panics_on_hostile_bytes() {
    let magic = minimal_svg();
    let header_len = magic.len();
    run_battery("thumb_svg::rasterize_svg", &magic, header_len, |b| {
        let r = rasterize_svg(b, 32);
        if b.is_empty() {
            assert!(r.is_err(), "rasterize_svg(empty bytes) must be Err, not a panic");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// SVG-specific mutations the generic byte battery can't reach: huge flat element counts and
// adversarial attribute values.
// ---------------------------------------------------------------------------------------------

#[test]
fn rasterize_svg_never_panics_on_a_huge_flat_element_count() {
    // Many SIBLING elements (not nested — this is a width/element-count stress, not the depth probe
    // below), aimed well under usvg's own documented 1,000,000-element cap but far past anything a real
    // thumbnail-sized SVG would ever contain.
    for &n in &[10_000usize, 50_000] {
        let mut doc = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
        for i in 0..n {
            doc.push_str(&format!(r##"<rect x="{i}" y="0" width="1" height="1" fill="#f00"/>"##));
        }
        doc.push_str("</svg>");
        let bytes = doc.into_bytes();
        assert_no_panic("thumb_svg::rasterize_svg", &format!("flat_element_count_{n}"), || {
            let _ = rasterize_svg(&bytes, 32);
        });
    }
}

#[test]
fn rasterize_svg_never_panics_on_adversarial_attribute_values() {
    let bodies: Vec<(&str, String)> = vec![
        ("nan_width", r##"<svg xmlns="http://www.w3.org/2000/svg" width="NaN" height="10"><rect width="10" height="10" fill="#f00"/></svg>"##.to_string()),
        ("infinity_width", r##"<svg xmlns="http://www.w3.org/2000/svg" width="Infinity" height="10"><rect width="10" height="10" fill="#f00"/></svg>"##.to_string()),
        ("negative_width_height", r##"<svg xmlns="http://www.w3.org/2000/svg" width="-10" height="-10"><rect width="10" height="10" fill="#f00"/></svg>"##.to_string()),
        ("zero_width_height", r##"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"><rect width="10" height="10" fill="#f00"/></svg>"##.to_string()),
        ("empty_width_height", r##"<svg xmlns="http://www.w3.org/2000/svg" width="" height=""><rect width="10" height="10" fill="#f00"/></svg>"##.to_string()),
        ("scientific_notation_huge", r##"<svg xmlns="http://www.w3.org/2000/svg" width="1e30" height="1e30"><rect width="10" height="10" fill="#f00"/></svg>"##.to_string()),
        ("garbage_viewbox", r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="not a viewbox"><rect width="10" height="10" fill="#f00"/></svg>"##.to_string()),
        ("negative_viewbox_size", r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 -100 -100"><rect width="10" height="10" fill="#f00"/></svg>"##.to_string()),
        ("nan_viewbox", r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="NaN NaN NaN NaN"><rect width="10" height="10" fill="#f00"/></svg>"##.to_string()),
        ("malformed_transform", r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#f00" transform="matrix(not,valid,here,at,all,zzz)"/></svg>"##.to_string()),
        ("huge_stroke_width", r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#f00" stroke="#000" stroke-width="999999999999"/></svg>"##.to_string()),
        ("percent_units_without_viewbox", r##"<svg xmlns="http://www.w3.org/2000/svg" width="50%" height="50%"><rect width="100%" height="100%" fill="#f00"/></svg>"##.to_string()),
        ("mismatched_units", r##"<svg xmlns="http://www.w3.org/2000/svg" width="10cm" height="10in"><rect width="10" height="10" fill="#f00"/></svg>"##.to_string()),
        ("garbage_fill_color", r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="not-a-color"/></svg>"##.to_string()),
        ("very_long_attribute_value", format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#f00" data-x="{}"/></svg>"##,
            "a".repeat(200_000)
        )),
    ];

    for (name, body) in bodies {
        let bytes = body.into_bytes();
        assert_no_panic("thumb_svg::rasterize_svg", name, || {
            let _ = rasterize_svg(&bytes, 32);
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Stack-overflow probes: run on a dedicated 256KB thread stack so a real overflow is detected via the
// thread's `.join()` failing (or the whole process aborting), never via `catch_unwind` (which cannot
// catch a stack overflow — it's not a panic).
// ---------------------------------------------------------------------------------------------

const SMALL_STACK: usize = 256 * 1024;

/// Run `f` on a dedicated `SMALL_STACK`-byte thread and assert it completed (returned or panicked
/// gracefully) rather than crashing the whole process with a stack overflow. Returns whatever `f`
/// returns, for further assertions by the caller.
fn run_on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let handle = std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(f)
        .expect("failed to spawn the small-stack probe thread");
    handle.join().expect(
        "rasterize_svg must not crash/overflow the stack on a small thread — a stack overflow is \
         uncatchable and aborts the whole process, so seeing this panic message at all (rather than a \
         raw STATUS_STACK_OVERFLOW/SIGSEGV process crash) would itself already mean the probe thread's \
         own harness code panicked, not `rasterize_svg`",
    )
}

#[test]
fn rasterize_svg_never_stack_overflows_on_deep_nesting_on_a_small_stack() {
    // CPE-1413's core finding + fix: before the fix, this reliably crashed the whole process with an
    // uncatchable stack overflow well under 500 levels of `<g>` nesting on a 256KB stack (confirmed via a
    // throwaway probe during this investigation). `rasterize_svg`'s new `xml_nesting_too_deep` pre-scan
    // now rejects nesting this deep before it ever reaches `usvg`/`roxmltree`, so this must come back as
    // a graceful `Err`.
    let bytes = deeply_nested_svg(4000);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "implausibly deep SVG nesting must be rejected, not risk a stack overflow");
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_use_self_reference_on_a_small_stack() {
    // `<use xlink:href="#self">` referencing itself — usvg explicitly detects `link == node` and skips
    // the element with a warning rather than recursing, so this is safe by construction, but it's exactly
    // the shape the ticket asks to probe, so it's asserted here as a small-stack regression guard.
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
        <use id="self" xlink:href="#self"/>
    </svg>"##
        .to_vec();
    let result = run_on_small_stack(move || rasterize_svg(&svg, 32));
    // A self-referencing `use` with nothing else to draw legitimately renders as an empty (but valid)
    // image — assert only "didn't crash", not a specific Ok/Err, matching this harness's usual philosophy
    // of not over-asserting on adversarial-but-not-unambiguous input.
    let _ = result;
}

#[test]
fn rasterize_svg_use_mutual_reference_cycle_is_now_rejected_gracefully() {
    // Formerly a known, `#[ignore]`d issue (CPE-1414): this mutual `<symbol>` reference cycle (two
    // <symbol>s each referencing the other via xlink:href) reliably stack-overflowed and crashed the
    // whole test process on a 256KB thread stack — usvg only guards direct self-reference and one-hop
    // parent/sibling back-references, so a 2-hop cycle fell through to its `depth > 1024` recursion cap,
    // whose own per-level stack cost was too high for a small stack. Now caught by CPE-1437's
    // `use_reference_chain_too_deep` guard: its non-recursive DFS treats a node revisited while still
    // `InProgress` on the current walk as a cycle (an unbounded chain by construction) and rejects it up
    // front, before `usvg` ever sees the bytes — so this must now come back as a graceful `Err`, not just
    // "didn't crash".
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
        <symbol id="a"><use xlink:href="#b"/></symbol>
        <symbol id="b"><use xlink:href="#a"/></symbol>
        <use xlink:href="#a"/>
    </svg>"##
        .to_vec();
    let result = run_on_small_stack(move || rasterize_svg(&svg, 32));
    assert!(result.is_err(), "a mutual <use>/<symbol> reference cycle must be rejected, not risk a stack overflow");
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_deep_acyclic_use_chain_on_a_small_stack() {
    // CPE-1437's core finding + fix: a flat, ACYCLIC chain of ~500 `<use>` elements each referencing the
    // previous passes the deep-nesting guard untouched (siblings, ~1 level deep in the raw XML) and isn't
    // a reference cycle either, yet before the fix this reliably crashed the whole process with an
    // uncatchable stack overflow on a 256KB stack (usvg resolves each hop via recursive cloning, so
    // resolution depth scales with chain length). `rasterize_svg`'s new `use_reference_chain_too_deep`
    // pre-scan now rejects a chain this deep before it ever reaches `usvg`/`roxmltree`'s own resolution,
    // so this must come back as a graceful `Err`.
    let bytes = flat_use_chain_svg(500);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "an implausibly deep <use> reference chain must be rejected, not risk a stack overflow");
}

#[test]
fn rasterize_svg_renders_a_shallow_use_chain_fine_on_a_small_stack() {
    // The chain-depth cap must not over-reject realistic, shallow `<use>` indirection (a handful of hops
    // is normal for icon-sprite-sheet-style SVGs) — confirmed here on the same small-stack probe used by
    // the adversarial cases above, so this doubles as a "the guard isn't just rejecting everything" check.
    let bytes = flat_use_chain_svg(3);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_ok(), "a shallow, legitimate <use> chain must still render: {result:?}");
}

// ---------------------------------------------------------------------------------------------
// CPE-1437 attempt 2: the composition bypass + the pre-existing CPE-1413 conversion-depth hole, both
// closed by `rasterize_svg` now doing its real work on a dedicated large-stack thread rather than relying
// on depth-prediction guards. Each of these payloads passes BOTH `xml_nesting_too_deep` and
// `use_reference_chain_too_deep` untouched, yet reliably crashed the 256KB probe before this fix.
// ---------------------------------------------------------------------------------------------

#[test]
fn rasterize_svg_never_stack_overflows_on_a_composed_chain_of_nested_use_containers_on_a_small_stack() {
    // The auditor's core "composition" finding: usvg's real native recursion cost is [<use>-hop count] x
    // [each hop's target's own internal <g>-nesting depth] — not either dimension alone. This payload (10
    // containers, each <g>-nested 20 deep, chained by <use> hops — ~11 hops / ~22 max local nesting) passes
    // both existing fast-reject guards individually, yet reliably overflowed the 256KB probe under CPE-1437
    // attempt 1 (a depth-prediction guard that didn't bound the product). Fixed for real in attempt 2:
    // `rasterize_svg` now does its actual work on a dedicated large-stack thread, so this composed payload
    // must now render successfully rather than merely fail gracefully — it's a legitimate shape well within
    // `usvg`'s own 1024-level cap.
    let bytes = composed_use_chain_with_nested_containers_svg(10, 20);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(
        result.is_ok(),
        "a composed use-chain-through-nested-containers payload must render, not overflow: {result:?}"
    );
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_single_deeply_nested_use_target_on_a_small_stack() {
    // The single-hop variant of the same composition bug: ONE <use> pointing at a single 40-deep <g>
    // container (1 hop / ~42 max local nesting) — trivially passes both existing caps, but the target's
    // own literal nesting alone (well under the 64 nesting cap) still overflowed the 256KB probe during
    // usvg's tree-conversion pass under attempt 1. Confirms the guaranteed-large-stack fix isn't just
    // masking the multi-container case above.
    let bytes = composed_use_chain_with_nested_containers_svg(1, 40);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_ok(), "a single <use> into a deeply-nested container must render, not overflow: {result:?}");
}

#[test]
fn rasterize_svg_never_stack_overflows_on_plain_nesting_under_the_cap_on_a_small_stack() {
    // The pre-existing CPE-1413 hole the audit surfaced: plain <g> nesting with NO <use> at all overflowed
    // the 256KB probe around depth ~35 during usvg's tree-*conversion* pass (a separate, heavier recursive
    // pass than the raw XML parse `xml_nesting_too_deep` was originally profiled against) — UNDER that
    // guard's existing cap of 64, so this shape sailed through untouched and still crashed. The
    // guaranteed-large-stack fix makes this a non-issue regardless of where usvg's real crash threshold for
    // any given internal recursive pass sits, present or future.
    let bytes = deeply_nested_svg(35);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_ok(), "35 levels of plain <g> nesting (under the existing cap) must render, not overflow: {result:?}");
}

// ---------------------------------------------------------------------------------------------
// Legitimate SVGs, confirmed to still render fine on the small-stack probe now that `rasterize_svg`
// provides its own large stack rather than depending on the caller's.
// ---------------------------------------------------------------------------------------------

#[test]
fn rasterize_svg_renders_a_symbol_sprite_sheet_fine_on_a_small_stack() {
    // A realistic "icon sprite sheet": one <symbol> definition referenced by several <use>s.
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="30" height="10">
        <symbol id="icon" viewBox="0 0 10 10"><rect width="10" height="10" fill="#f00"/></symbol>
        <use href="#icon" x="0" width="10" height="10"/>
        <use href="#icon" x="10" width="10" height="10"/>
        <use href="#icon" x="20" width="10" height="10"/>
    </svg>"##
        .to_vec();
    let result = run_on_small_stack(move || rasterize_svg(&svg, 32));
    assert!(result.is_ok(), "a legitimate icon sprite sheet must render: {result:?}");
}

#[test]
fn rasterize_svg_renders_an_eight_deep_grouped_illustration_fine_on_a_small_stack() {
    // A realistic, moderately-grouped illustration (layers/groups nested a handful of levels, well under
    // any cap and well under the ~35-level conversion-pass crash threshold the audit found).
    let bytes = deeply_nested_svg(8);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_ok(), "a realistic 8-deep grouped illustration must render: {result:?}");
}

// ---------------------------------------------------------------------------------------------
// CPE-1437 attempt 3: `usvg` reference-resolution recursions bounded only by total element count
// (~1,000,000), NOT by its 1024-level `<use>` cap — so no stack size closes them, only bounding the
// INPUT does. Each of these is confirmed (per the module doc comment) to overflow even the 16MiB
// guaranteed-stack thread at a large-but-plausible N; `reference_chain_too_deep`'s generalized walk must
// now reject all of them gracefully well before that, while a legitimate single-level use of each still
// renders fine.
// ---------------------------------------------------------------------------------------------

#[test]
fn rasterize_svg_never_stack_overflows_on_a_clippath_chain_on_a_small_stack() {
    // Confirmed reproducer: Ok at N=7000, STATUS_STACK_OVERFLOW at N=8000 even through the 16MiB thread
    // (usvg's `clippath::convert` self-recurses with no depth cap). N=8000 here, well above that floor.
    let bytes = clip_path_chain_svg(8000);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "an implausibly deep clipPath chain must be rejected, not risk a stack overflow");
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_mask_chain_on_a_small_stack() {
    // Confirmed reproducer: overflows even the 16MiB thread by N=5000 (usvg's `mask::convert` has the
    // identical unbounded self-recursion as clipPath). N=8000 here, comfortably above that floor.
    let bytes = mask_chain_svg(8000);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "an implausibly deep mask chain must be rejected, not risk a stack overflow");
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_pattern_chain_on_a_small_stack() {
    // `pattern` content converts through the same general converter as everything else, so a
    // pattern-inside-a-pattern chain has the same unbounded-recursion shape as clipPath/mask.
    let bytes = pattern_chain_svg(8000);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "an implausibly deep pattern chain must be rejected, not risk a stack overflow");
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_marker_chain_on_a_small_stack() {
    // Same reasoning as pattern, for `marker` content and `marker-start`.
    let bytes = marker_chain_svg(8000);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "an implausibly deep marker chain must be rejected, not risk a stack overflow");
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_filter_feimage_chain_on_a_small_stack() {
    // The indirect filter<->feImage<->element<->filter chain — bounded the same way via
    // `direct_reference_targets`' `<feImage>` handling and `hops_from_target` scanning a `<filter>`'s own
    // subtree for its `feImage` children.
    let bytes = filter_feimage_chain_svg(8000);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(
        result.is_err(),
        "an implausibly deep filter/feImage reference chain must be rejected, not risk a stack overflow"
    );
}

#[test]
fn rasterize_svg_renders_a_legit_single_level_clip_path_mask_pattern_marker_filter_fine_on_a_small_stack() {
    // The chain-depth cap must not over-reject ordinary, single-level use of each of the five newly-bounded
    // reference types — this is the "isn't just rejecting everything" check for attempt 3, mirroring
    // `rasterize_svg_renders_a_shallow_use_chain_fine_on_a_small_stack` above.
    let cases: [(&str, Vec<u8>); 5] = [
        ("clip_path", clip_path_chain_svg(1)),
        ("mask", mask_chain_svg(1)),
        ("pattern", pattern_chain_svg(1)),
        ("marker", marker_chain_svg(1)),
        ("filter_feimage", filter_feimage_chain_svg(1)),
    ];
    for (name, bytes) in cases {
        let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
        assert!(result.is_ok(), "a legitimate single-level '{name}' reference must still render: {result:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// CPE-1444: the MULTIPLICATIVE composition vector attempt 3 flagged but left open. For `mask`/`pattern`/
// `marker` (and `filter`/`feImage`), usvg descends each hop's OWN literal `<g>` nesting on-stack while the
// reference-chain recursion frame is still live, so real recursion cost ≈ hops × nesting. Each of these
// reproducers passes BOTH the hop cap (127 < 128) and the per-document nesting cap (== 64) yet drives usvg
// to ≈7874 frames (release floor; debug floor ≈4500) — overflowing even the 16MiB render thread. The
// combined-cost cap (MAX_REFERENCE_COMBINED_COST) now rejects them well before that, while a legit
// multi-level mask/pattern/marker still renders.
// ---------------------------------------------------------------------------------------------

#[test]
fn rasterize_svg_never_stack_overflows_on_a_nested_mask_composition_on_a_small_stack() {
    // 127 mask hops × 62-deep <g> per hop ≈ 7874 frames — overflows the 16MiB thread on release and the
    // ~4500-frame debug floor CI builds against. Must be a graceful Err via the combined-cost cap.
    let bytes = mask_nested(127, 62);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "a nested (hops × nesting) mask composition must be rejected, not overflow");
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_nested_pattern_composition_on_a_small_stack() {
    let bytes = pattern_nested(127, 62);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "a nested (hops × nesting) pattern composition must be rejected, not overflow");
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_nested_marker_composition_on_a_small_stack() {
    let bytes = marker_nested(127, 62);
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "a nested (hops × nesting) marker composition must be rejected, not overflow");
}

#[test]
fn rasterize_svg_combined_cost_boundary_just_under_renders_just_over_is_rejected() {
    // Per-hop cost is `nest`+2 = 64 at nest=62, so the 2048 cap is reached at exactly 32 hops (allowed) and
    // exceeded at 33 (rejected). The just-under case (32×64 = 2048 frames, well under the ~4500 debug
    // overflow floor) must actually RENDER; the just-over case must be a graceful Err.
    let under = mask_nested(32, 62);
    let under_result = run_on_small_stack(move || rasterize_svg(&under, 32));
    assert!(under_result.is_ok(), "a chain exactly at the combined-cost cap must still render: {under_result:?}");

    let over = mask_nested(33, 62);
    let over_result = run_on_small_stack(move || rasterize_svg(&over, 32));
    assert!(over_result.is_err(), "one hop past the combined-cost cap must be rejected, not risk an overflow");
}

#[test]
fn rasterize_svg_renders_legit_multi_level_mask_pattern_marker_within_the_cap_on_a_small_stack() {
    // The "isn't just rejecting everything" check for CPE-1444: a realistic layered-effect SVG — a handful
    // of mask/pattern/marker hops, a few groups deep — is far under the product cap and must render.
    let cases: [(&str, Vec<u8>); 3] = [
        ("mask", mask_nested(4, 8)),
        ("pattern", pattern_nested(4, 8)),
        ("marker", marker_nested(4, 8)),
    ];
    for (name, bytes) in cases {
        let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
        assert!(
            result.is_ok(),
            "a legitimate multi-level '{name}' composition within the cap must render: {result:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// CPE-1445: SVGZ (gzip-compressed .svg). `xml_nesting_too_deep` is a pure byte scan with no gzip
// awareness — handed the RAW (still-compressed) bytes of a gzipped SVG, it sees no '<' tags at all and
// silently reports "not too deep" regardless of what the document actually decompresses to (a guard
// BYPASS, not merely a miss), and gzip decompression itself (both `usvg`'s own internal
// `decompress_svgz` and, before this fix, `reference_chain_too_deep`'s own call to it) had no size cap,
// letting a tiny crafted stream force a huge allocation. `rasterize_svg` now decompresses SVGZ input
// once, bounded, up front, before any guard runs — see `thumb_svg::decompress_svgz_bounded` and the
// module doc comment's "CPE-1445" section.
// ---------------------------------------------------------------------------------------------

/// Gzip-compresses `content` with a real `flate2::write::GzEncoder` — the same construction
/// `archive_panic_safety.rs`'s `build_valid_gz` uses. `flate2` is already a normal dependency of
/// `cpe-server` (used throughout `archive.rs`/`thumb_font.rs`/`thumb_svg.rs` itself), so it's directly
/// available to this integration test too; no new dependency.
fn gzip_bytes(content: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
    enc.write_all(content).unwrap();
    enc.finish().unwrap();
    out
}

/// Builds a gzip stream that DECOMPRESSES to `logical_size` bytes of zeros — the classic "gzip bomb"
/// shape (highly compressible content, so a tiny compressed stream claims/produces a huge decompressed
/// size) — without ever holding `logical_size` bytes in memory at once: the encoder is fed a small fixed
/// chunk of zeros repeatedly rather than one giant buffer, mirroring
/// `archive_panic_safety.rs`'s streamed-fixture style for the same reason (building the fixture itself
/// must stay cheap even when `logical_size` is chosen well past any cap this test wants to prove is
/// enforced).
fn gzip_bomb(logical_size: usize) -> Vec<u8> {
    const CHUNK: usize = 1024 * 1024;
    let zeros = vec![0u8; CHUNK];
    let mut out = Vec::new();
    let mut enc = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
    let mut written = 0usize;
    while written < logical_size {
        let take = CHUNK.min(logical_size - written);
        enc.write_all(&zeros[..take]).unwrap();
        written += take;
    }
    enc.finish().unwrap();
    out
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_gzipped_deeply_nested_svg_on_a_small_stack() {
    // CPE-1445's core finding: the exact same 4000-deep `<g>` nesting
    // `rasterize_svg_never_stack_overflows_on_deep_nesting_on_a_small_stack` (above) proves is rejected
    // for a PLAIN `.svg` must be rejected here too when the bytes are gzip-compressed (an SVGZ file) —
    // before this fix, `xml_nesting_too_deep` saw only opaque compressed bytes (no '<' tags) and silently
    // let this straight through to `usvg`/`roxmltree` on the small probe stack.
    let bytes = gzip_bytes(&deeply_nested_svg(4000));
    assert!(bytes.starts_with(&[0x1f, 0x8b]), "fixture must actually be gzip-compressed (SVGZ)");
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "a gzipped implausibly-deep SVG must be rejected, not risk a stack overflow");
}

#[test]
fn rasterize_svg_rejects_a_gzip_bomb_without_unbounded_decompression() {
    // CPE-1445's second finding: gzip decompression itself was uncapped, so a tiny compressed stream that
    // decompresses to a huge size would make `rasterize_svg` try to allocate that much before any other
    // guard got a chance to run. Built here as a stream that decompresses to 200 MiB of zeros —
    // comfortably over the 32 MiB `MAX_DECOMPRESSED_SVG_BYTES` cap — while the COMPRESSED fixture itself
    // stays tiny (>1000x compression ratio), proving the CAP is what stops the inflation, not incidental
    // slowness: this must return promptly with a graceful `Err`, never attempt to materialize the full
    // 200 MiB (let alone a true multi-GB bomb, which this deliberately does not attempt to reproduce in
    // CI — the bounded `.take()` in `decompress_svgz_bounded` means the cap is provably size-independent
    // of how large the stream's true logical payload is).
    let logical_size = 200 * 1024 * 1024;
    let bytes = gzip_bomb(logical_size);
    assert!(
        bytes.len() < logical_size / 1000,
        "fixture should compress by at least 1000x to be a real bomb-ish case, got {} bytes for a {} \
         logical payload",
        bytes.len(),
        logical_size
    );
    let result = rasterize_svg(&bytes, 32);
    assert!(result.is_err(), "a gzip stream that would decompress past the cap must be rejected, not OOM");
}

#[test]
fn rasterize_svg_renders_a_legit_small_gzipped_svg_fine() {
    // A real, small SVGZ file (well under every cap) must still rasterize normally — the fix must not
    // over-reject legitimate gzip-compressed SVG input.
    let bytes = gzip_bytes(&minimal_svg());
    let result = rasterize_svg(&bytes, 32);
    assert!(result.is_ok(), "a legitimate small gzipped SVG (SVGZ) must still render: {result:?}");
}

#[test]
fn rasterize_svg_renders_a_plain_uncompressed_svg_fine_after_the_gzip_fixes() {
    // Regression: neither the single- nor double-gzip guards should touch ordinary, never-compressed
    // `.svg` input at all (it never starts with the `1F 8B` magic).
    let bytes = minimal_svg();
    let result = rasterize_svg(&bytes, 32);
    assert!(result.is_ok(), "a plain uncompressed SVG must still render fine: {result:?}");
}

// ---------------------------------------------------------------------------------------------
// CPE-1445 attempt 2: an independent adversarial re-audit of the fix above found that a single bounded
// decompress does not, by itself, guarantee "the bytes usvg receives are never gzip-magic'd" — a
// DOUBLY (or N-fold) gzipped `.svg` peels only the outer layer via `decompress_svgz_bounded`, and the
// result still starts with `1F 8B` (the untouched inner gzip stream). Before this fix, those
// still-compressed bytes flowed straight into `usvg::Tree::from_data`, which re-detects the magic and
// decompresses the inner layer itself with NO cap of its own — reopening both the nesting-guard-bypass
// and the decompression-OOM sub-bugs one gzip layer down. `rasterize_svg` now rejects outright the
// moment a decompress still leaves the gzip magic in place (a legitimate SVGZ is always exactly one
// gzip layer), with a second defense-in-depth check right at the `usvg::Tree::from_data` boundary.
// ---------------------------------------------------------------------------------------------

#[test]
fn rasterize_svg_never_stack_overflows_on_a_doubly_gzipped_deeply_nested_svg_on_a_small_stack() {
    // The exact CPE-1445-attempt-2 reproducer: gzip(gzip(deeply_nested_svg(4000))). Before this fix, the
    // outer bounded decompress peeled one layer, found the result still gzip-magic'd, and (with no
    // reject) handed it straight to `usvg::Tree::from_data`, which decompressed the inner layer itself —
    // unbounded — and fed 4000-deep `<g>` nesting to `roxmltree`/usvg's conversion pass on the 16MiB
    // render thread, overflowing it (STATUS_STACK_OVERFLOW, uncatchable, crashes the whole process). Must
    // now be a graceful `Err`.
    let bytes = gzip_bytes(&gzip_bytes(&deeply_nested_svg(4000)));
    assert!(bytes.starts_with(&[0x1f, 0x8b]), "outer layer must itself be valid gzip");
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(
        result.is_err(),
        "a doubly-gzipped implausibly-deep SVG must be rejected, not risk a stack overflow"
    );
}

#[test]
fn rasterize_svg_never_stack_overflows_on_a_triply_gzipped_deeply_nested_svg_on_a_small_stack() {
    // N-fold gzip (N=3) must be rejected the same way as N=2 — the guard checks the magic after ONE
    // decompress and rejects unconditionally, so it doesn't matter how many further layers remain.
    let bytes = gzip_bytes(&gzip_bytes(&gzip_bytes(&deeply_nested_svg(4000))));
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_err(), "a triply-gzipped implausibly-deep SVG must be rejected, not risk a stack overflow");
}

#[test]
fn rasterize_svg_rejects_a_doubly_gzipped_bomb_without_unbounded_inner_decompression() {
    // The CPE-1445-attempt-2 OOM reproducer: gzip(gzip_bomb(200MiB)). The OUTER bounded decompress alone
    // only bounds the outer layer's output size — before this fix, that output (the still-compressed
    // 200MiB-logical inner gzip stream) sailed through to `usvg::Tree::from_data`, which decompressed the
    // FULL inner layer unbounded (confirmed by the coordinator's re-audit: peak ~270MiB for a <1MB file).
    // Must now be a graceful `Err`, never attempting the inner inflation at all.
    let logical_size = 200 * 1024 * 1024;
    let inner_bomb = gzip_bomb(logical_size);
    let bytes = gzip_bytes(&inner_bomb);
    assert!(
        bytes.len() < logical_size / 1000,
        "fixture should compress by at least 1000x end-to-end to be a real bomb-ish case, got {} bytes \
         for a {} logical payload",
        bytes.len(),
        logical_size
    );
    let result = rasterize_svg(&bytes, 32);
    assert!(
        result.is_err(),
        "a doubly-gzipped stream whose inner layer would decompress past the cap must be rejected, not OOM"
    );
}

#[test]
fn rasterize_svg_renders_a_legit_single_gzip_svgz_fine_after_the_double_gzip_guard() {
    // The "isn't just rejecting everything" check for attempt 2: ordinary single-layer SVGZ input (the
    // only shape a real SVG authoring/export tool ever produces) must still render normally — the new
    // still-gzip-magic'd-after-one-decompress reject must fire on multi-layer input only, never on a
    // single legitimate layer.
    let bytes = gzip_bytes(&minimal_svg());
    let result = run_on_small_stack(move || rasterize_svg(&bytes, 32));
    assert!(result.is_ok(), "a legitimate single-layer SVGZ file must still render: {result:?}");
}
