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
//!   `#u500`) is ALSO a real, confirmed stack-overflow DoS on a small stack** (CPE-1437) — it passes BOTH
//!   the deep-nesting guard above (siblings are only ~1 deep in the raw XML) and would not be flagged by a
//!   cycle-only guard (it's genuinely acyclic), yet `usvg` still resolves it via one recursive clone per
//!   link, so resolution depth scales with chain length. **Fixed**: `thumb_svg::use_reference_chain_too_deep`
//!   bounds this reference-chain depth with its own non-recursive graph walk, run before `usvg` ever sees
//!   the bytes. [`rasterize_svg_never_stack_overflows_on_a_deep_acyclic_use_chain_on_a_small_stack`] below
//!   is the regression test for the fix.

mod common;
use common::{assert_no_panic, run_battery};

use cpe_server::thumb_svg::rasterize_svg;

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
