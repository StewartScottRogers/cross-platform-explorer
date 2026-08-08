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
//! **A subtlety this fix's own verification caught:** [`reference_chain_too_deep`] is *not* the same
//! kind of "cheap, provably non-recursive" check [`xml_nesting_too_deep`] is — to mirror `usvg`'s exact
//! entity/DTD decoding (see that function's own doc comment) it calls the REAL, recursive
//! `roxmltree::Document::parse_with_options`, the identical parser class CPE-1413 originally found recurses
//! per XML nesting level with no cap of its own. An early version of this attempt left that call on the
//! *caller's* thread (reasoning it ran "before" the big-stack spawn, so it'd only ever see shallow input
//! that already passed `xml_nesting_too_deep`'s 64-level cap) — but a small-stack diagnostic during
//! verification showed a BARE `roxmltree` parse of a document only ~42 levels deep (comfortably under that
//! 64-level cap) already overflows a 256KB stack by itself. So `reference_chain_too_deep` now runs
//! *inside* [`rasterize_svg_on_a_guaranteed_stack`]'s closure, on the same guaranteed stack as the render —
//! only [`xml_nesting_too_deep`]'s genuinely flat byte-scan loop is safe to run on the caller's own thread.
//!
//! **CPE-1437 attempt 3 — the 16MiB stack (attempt 2) closes the `<use>`/nesting composition class, since
//! that's bounded by `usvg`'s own 1024-level recursion cap either way, but it does NOT close everything:**
//! an independent adversarial audit found `usvg` has SEVERAL OTHER reference-resolution recursions that are
//! bounded only by total element count (~1,000,000), not by the 1024 cap — so a long enough ACYCLIC chain
//! of them overflows even a large guaranteed stack, and "no fixed stack size closes it: a longer chain
//! always exists." Confirmed by reading `usvg-0.45.1`'s own source (`parser/clippath.rs`, `parser/mask.rs`,
//! `parser/paint_server.rs`, `parser/marker.rs`, `parser/filter.rs`) and empirically reproduced:
//! - `clippath::convert` self-recursively calls `convert(link, ...)` when a `<clipPath clip-path="url(#…)">`
//!   itself has a `clip-path`. **Reproducer: a chain of `N` clipPaths, `Ok` at N=7000, `STATUS_STACK_OVERFLOW`
//!   at N=8000 even through the 16MiB thread** (~686KB SVG).
//! - `mask::convert` does the identical self-recursion for `<mask mask="url(#…)">`. **Overflows by N=5000**
//!   (~413KB SVG).
//! - `paint_server::convert_pattern` converts a `<pattern>`'s content via the SAME general
//!   `converter::convert_children` used for every other element's children — so a shape *inside* a pattern
//!   that itself has `fill="url(#anotherPattern)"` recurses back into `convert_pattern` via ordinary mutual
//!   recursion through the converter, not a dedicated self-call, but the effect (and the missing depth cap)
//!   is identical.
//! - `marker::convert` converts a `<marker>`'s content the same way (`converter::convert_children`), so a
//!   path inside a marker with its own `marker-start="url(#anotherMarker)"` recurses the same way.
//! - `filter::convert`/`convert_url` don't chain filter-to-filter directly, but a `<filter>` can contain a
//!   `<feImage href="#element">` referencing an ARBITRARY element via `converter::convert_element` — so a
//!   `<filter>` → `<feImage href>` an element with its OWN `filter="url(#anotherFilter)"` → that filter's own
//!   `feImage` → ... chain recurses through the exact same converter mutual recursion.
//! - `usvg`'s only defense against any of these (`fix_recursive_links` / the various `link == node`
//!   self-reference checks in each `convert()`) breaks a **direct 1-hop cycle**, not a long **acyclic**
//!   chain — the identical gap CPE-1414's cycle-only guard had for `<use>`, just never fixed for these five
//!   reference types. **This DoS already existed on `main` before CPE-1437 ever started** (a 2MB prod stack
//!   overflows at an even shorter chain than the 256KB probe does) — CPE-1437 didn't introduce it, but
//!   closing the small-stack bar means closing this too.
//!
//! Since no stack size can bound an unbounded-by-`usvg`-itself chain, [`reference_chain_too_deep`] (renamed
//! from `use_reference_chain_too_deep`) is generalized to walk ALL SIX of these reference-resolution edge
//! types in one unified graph — `<use>`/`<feImage>` `href`, plus `clip-path`/`mask`/`filter`/
//! `marker-start`/`marker-mid`/`marker-end`/`fill`/`stroke` `url(#id)` references — under one combined depth
//! cap ([`MAX_REFERENCE_CHAIN_DEPTH`], renamed from `MAX_USE_CHAIN_DEPTH`, value unchanged at 128). See
//! [`direct_reference_targets`] and [`hops_from_target`] for how each edge type is resolved (reusing
//! [`resolve_use_href`] for the bare-IRI `href` cases and a new [`find_func_iri_ids`] — mirroring
//! `svgtypes::FuncIRI::from_str`'s grammar the same way [`parse_iri_fragment`] mirrors `IRI::from_str` — for
//! the `url(#id)` cases). The 16MiB guaranteed stack from attempt 2 is kept as defense-in-depth for whatever
//! remains bounded by `usvg`'s own 1024 cap (the `<use>`/nesting composition class).
//!
//! **CPE-1437 attempt 3's residual risk — now CLOSED by CPE-1444:** attempt 3 flagged, but did not close,
//! a *combined* attack interleaving these reference types with independent literal `<g>` nesting inside each
//! hop. That gap was real and exploitable: for the MULTIPLICATIVE reference types (`mask`, `filter`/
//! `feImage`, `fill`/`stroke`-referenced `pattern`, `marker-*`), `usvg` descends each hop target's own
//! literal `<g>`/element nesting *while the reference-chain recursion frame is still on the stack*, so its
//! real native recursion depth is neither the hop count alone (bounded by [`MAX_REFERENCE_CHAIN_DEPTH`]=128)
//! nor a single document's own nesting alone (bounded by [`MAX_SVG_NESTING_DEPTH`]=64) but roughly their
//! **product**. Concrete reproducers `mask_nested(127, 62)` / `pattern_nested(127, 62)` /
//! `marker_nested(127, 62)` — a chain of 127 hops where each hop's `url(#…)` sits at the bottom of 62 nested
//! `<g>` levels — pass BOTH independent caps (127 < 128 hops, each document's literal nesting == 64) yet
//! drive `usvg` to ≈127×64 ≈ 7874 stack frames, overflowing even the [`RASTERIZE_STACK_SIZE`] 16MiB thread
//! (`STATUS_STACK_OVERFLOW`); in a debug build the per-frame cost is higher, dropping the floor to
//! ≈127×35 ≈ 4500 frames (CI builds debug, so the guarded envelope overflowed there outright).
//!
//! **CPE-1444 adds the missing SECOND dimension to [`reference_chain_too_deep`]'s walk:** alongside the hop
//! depth, the same DFS now accumulates a **combined (product) cost** — `Σ over the chain of each
//! multiplicative hop target's own [`subtree_nesting_depth`]` — and rejects past [`MAX_REFERENCE_COMBINED_COST`]
//! (2048, sized well under both the ~7874 release and ~4500 debug overflow floors, and ~34x above any
//! legitimate few-hops-×-shallow-nesting artwork — see that constant's doc comment). `clip-path` stays
//! ADDITIVE (usvg resolves a clip chain separately from group descent, so its cost is the hop count alone —
//! it keeps only the [`MAX_REFERENCE_CHAIN_DEPTH`] cap) and `<use>` stays additive/node-capped (bounded by
//! usvg's ~1,000,000-node / 1024-`<use>`-depth caps and the 16MiB stack — the composition class attempt 2
//! closed); `filter`/`feImage` are treated as multiplicative because a `feImage href` resolves an arbitrary
//! element via the same general converter that descends its subtree on-stack, exactly like `mask`. See
//! [`func_iri_attr_is_multiplicative`], [`chain_edges`], and the nested-composition regression tests in
//! `tests/thumb_svg_panic_safety.rs`.
//!
//! **CPE-1445 — the raw-byte guard's SVGZ (gzip) blind spot, plus uncapped decompression:** an
//! independent adversarial sweep (during the CPE-1437/CPE-1444 investigation above) found that
//! [`xml_nesting_too_deep`] — the ONLY guard that ran on the caller's own thread, ahead of everything
//! else, specifically because it's cheap and provably non-recursive (see its own doc comment) — is a pure
//! byte scan for `<`/`>` and never had any gzip awareness: handed the RAW (still-compressed) bytes of a
//! `*.svg` file whose content happens to be gzip (an "SVGZ" file, valid and auto-detected by `usvg` via
//! its `1F 8B` magic), the scan sees no XML tags at all and returns `false` — the guard is silently
//! bypassed, not merely ineffective. Worse, [`reference_chain_too_deep`] (added by CPE-1437, below) DID
//! already gzip-decompress SVGZ input before its own walk, but via `usvg`'s own public
//! [`resvg::usvg::decompress_svgz`], which — like `usvg::Tree::from_data`'s own internal call to the same
//! function — is a bare `Read::read_to_end` on a `flate2::read::GzDecoder` with **no size cap at all**: a
//! tiny crafted gzip stream (e.g. `"A".repeat(4GB)` compressing to only a few MB) makes it allocate
//! multiple gigabytes. Both sub-bugs stay well under `thumb_source`'s 128 MiB raw-file-size gate, since
//! the crafted *file* itself (a small gzip stream) is tiny — only what it *decompresses to* is huge.
//!
//! Fixed by moving gzip handling to the very front of [`rasterize_svg`], once, for the whole function:
//! [`decompress_svgz_bounded`] detects the `1F 8B` magic and inflates with a hard
//! [`MAX_DECOMPRESSED_SVG_BYTES`] cap (closing the OOM half), and — critically — EVERY guard that follows
//! ([`xml_nesting_too_deep`] on the caller's thread, then [`reference_chain_too_deep`] and the real `usvg`
//! parse/render inside [`rasterize_svg_on_a_guaranteed_stack`]) now runs on those already-decompressed
//! plain-XML bytes (closing the nesting-guard-bypass half): a gzipped deeply-nested document can no longer
//! sail past [`xml_nesting_too_deep`] by looking like opaque binary. [`reference_chain_too_deep`]'s own
//! (now normally unreachable in the `rasterize_svg` path, since its input is pre-decompressed) gzip
//! branch is also switched from the uncapped `usvg::decompress_svgz` to [`decompress_svgz_bounded`],
//! purely as defense-in-depth for any future direct caller of that function.
//!
//! **CPE-1445 attempt 2 — a DOUBLY-gzipped `.svg` bypassed attempt 1's whole premise:** an independent
//! adversarial re-audit found that attempt 1's single bounded decompress does not, by itself, guarantee
//! "the bytes handed onward are plain XML" — for a `.svg` that is gzip-of-gzip (two compression layers
//! stacked), one bounded decompress peels only the OUTER layer, and the result still starts with `1F 8B`
//! (the untouched INNER gzip stream). Attempt 1's own doc comment claimed `usvg::Tree::from_data`'s
//! internal `decompress_svgz` "never fires a second time" past this point — true for single-gzip input,
//! **false** for double (or triple, or N-fold) gzip: those still-magic'd bytes flow straight through to
//! `usvg::Tree::from_data`, which re-detects `1F 8B` and decompresses the inner layer itself, with **no
//! cap of its own** — reopening BOTH CPE-1445 sub-bugs one layer down. Confirmed reproducers: a
//! doubly-gzipped 4000-deep `<g>` nesting document overflows the 256KB small-stack probe (the inner layer
//! never passes through [`xml_nesting_too_deep`] on real XML, since that guard only ever sees attempt 1's
//! single already-decompressed-but-still-gzip-magic'd bytes); a doubly-gzipped gzip bomb lets `usvg`'s
//! internal decompression inflate the untouched inner layer without limit.
//!
//! A legitimate SVGZ file is always exactly one gzip layer — that's what every SVG authoring/export tool
//! and the SVGZ format itself produces — so nested gzip has no legitimate use case to preserve. Rather
//! than looping the bounded decompress across layers (which only relocates "how many layers is too many"
//! to a new, equally arbitrary cap), [`rasterize_svg`] rejects outright: after the one bounded decompress,
//! if the result STILL starts with `1F 8B`, that's an immediate graceful `Err`, never handed onward. A
//! second, defense-in-depth check sits at the actual boundary into `usvg::Tree::from_data` inside
//! [`rasterize_svg_on_a_guaranteed_stack`] — a bytes-still-gzip-magic'd assertion that also returns a
//! graceful `Err` — so the invariant "usvg never receives gzip-magic'd bytes" is enforced at BOTH the
//! point it's established and the point it's relied upon, not just the former.

use image::{DynamicImage, RgbaImage};
use std::collections::HashMap;
use std::io::Read;

/// Same spirit as `thumb_source::MAX_IMAGE_DIMENSION` — an SVG's *declared* intrinsic size is
/// clamped to this before we ever allocate a canvas. Real SVG artwork is never this big.
const MAX_SVG_DIMENSION: u32 = 20_000;

/// The maximum size a gzip-compressed `.svg` (SVGZ) file's DECOMPRESSED bytes may reach before
/// [`rasterize_svg`] rejects it outright (CPE-1445 — see the module doc comment). Applied via a bounded
/// `Read::take` directly on the gzip stream (see [`decompress_svgz_bounded`]), not a check performed
/// after an unbounded read already ran to completion, so a gzip "decompression bomb" — a tiny compressed
/// stream whose embedded (attacker-controlled, unverified) length claims to inflate to gigabytes — can
/// never make this function allocate more than this cap's worth of memory, no matter how the input is
/// shaped or how small the compressed file on disk is.
///
/// 32 MiB, sized the same way `doc_text::MAX_DECOMPRESSED_PART_BYTES` (8 MiB, CPE-1446) is: comfortably
/// above anything a legitimate hand-authored or tool-exported SVG's XML text ever reaches — real-world
/// SVGs, even large detailed illustrations or icon-font sprite sheets, are almost always well under a few
/// hundred KB, so multi-MB SVG XML is already an unusual outlier — while staying well under
/// `thumb_source::MAX_SOURCE_FILE_BYTES` (128 MiB), the existing gate on the raw (compressed-or-not) file
/// size read from disk. That relationship matters: it means decompressing a legitimate SVGZ can never use
/// MORE memory than simply reading an equivalently-sized plain (uncompressed) `.svg` already would have —
/// this cap doesn't shrink the legitimate SVGZ use case, it only removes gzip's ability to turn a
/// kilobyte-scale file into a gigabyte-scale allocation.
const MAX_DECOMPRESSED_SVG_BYTES: u64 = 32 * 1024 * 1024;

/// Gzip-decompresses `bytes` (an SVGZ file's raw contents, already confirmed to start with the `1F 8B`
/// magic by the caller) with a hard cap on the decompressed size (CPE-1445): closes the uncapped-
/// decompression-OOM half of the SVGZ bug, where `usvg`'s own `decompress_svgz` (and, before this fix,
/// this module's [`reference_chain_too_deep`]) call `Read::read_to_end` directly on a
/// `flate2::read::GzDecoder` with no size limit at all, letting a tiny crafted gzip stream (e.g. a few MB
/// compressing `"A".repeat(4GB)`) allocate multiple gigabytes before any other guard ever gets a chance to
/// run.
///
/// Wraps the decoder in `Read::take(MAX_DECOMPRESSED_SVG_BYTES + 1)` — one byte PAST the cap, so a stream
/// that decompresses to exactly the cap (which must be allowed) is distinguishable from one that keeps
/// producing bytes past it (which must be rejected) — and reads that bounded adapter to completion.
/// `flate2`'s `GzDecoder` only inflates as much of the compressed input as the reader actually pulls from
/// it, so a `Read::take`-limited `read_to_end` can never materialize more than
/// `MAX_DECOMPRESSED_SVG_BYTES + 1` decompressed bytes in memory, regardless of how large the stream
/// would keep expanding to if read further — the bomb's "would-be" size never gets a chance to matter.
/// Graceful `Err` on either a genuinely malformed gzip stream or a decompressed size over the cap; never a
/// panic, and never the unbounded allocation.
fn decompress_svgz_bounded(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut limited = decoder.take(MAX_DECOMPRESSED_SVG_BYTES + 1);
    let mut decompressed = Vec::new();
    limited
        .read_to_end(&mut decompressed)
        .map_err(|e| format!("malformed gzip (SVGZ) data: {e}"))?;
    if decompressed.len() as u64 > MAX_DECOMPRESSED_SVG_BYTES {
        return Err(format!(
            "SVGZ decompressed size exceeds the {MAX_DECOMPRESSED_SVG_BYTES}-byte cap"
        ));
    }
    Ok(decompressed)
}

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

/// One outgoing reference-chain edge from a node: the hop it leads to, paired with the **per-hop cost**
/// taking that hop adds to the combined (product) cost (CPE-1444) — the target subtree's own nesting depth
/// for a multiplicative reference, `0` for an additive one. See [`chain_edges`].
type ChainEdge<'a> = (XmlNode<'a>, usize);

/// One frame of [`reference_chain_too_deep`]'s explicit-stack DFS: the node being visited, its outgoing
/// [`ChainEdge`]s, and the index of the next edge to explore. Factored into a `type` alias to keep the
/// walk's stack declaration readable (and to satisfy `clippy::type_complexity`).
type DfsFrame<'a> = (XmlNode<'a>, Vec<ChainEdge<'a>>, usize);

/// Namespace URI for `xlink:href`. Matters because `usvg`'s own attribute resolution (`resolve_href` in
/// usvg-0.45.1's `parser/svgtree/parse.rs`) checks the **`xlink:href`-namespaced attribute FIRST**, and
/// only falls back to the un-namespaced `href` if that one is absent. This exact precedence is one of the
/// three bypass classes the CPE-1414 investigation found in earlier (never-shipped) guard attempts: a
/// guard that checked `href` before `xlink:href`, or matched by local attribute name only, could disagree
/// with `usvg` about which target an element with *both* attributes present actually resolves to (e.g.
/// `<use href="#leaf" xlink:href="#other"/>` — the guard would see `#leaf`, `usvg` resolves `#other`).
/// [`resolve_use_href`] reuses this exact order to close that class here too.
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";

/// The deepest **combined reference chain** ([`reference_chain_too_deep`]'s walk, over ALL SIX edge types —
/// `<use>`/`<feImage>` `href` plus `clip-path`/`mask`/`filter`/`marker-*`/`fill`/`stroke` `url(#id)` — see
/// the module doc comment's "attempt 3" section) allowed before `rasterize_svg` refuses the document
/// outright (CPE-1437). `usvg` resolves each hop by *recursion* (cloning for `<use>`, mutual recursion
/// through the general converter for the other five), so resolution stack depth scales with chain length;
/// unlike [`MAX_SVG_NESTING_DEPTH`] (literal XML nesting), a flat sibling chain of reference elements each
/// pointing at the previous is only ~1 level deep in the raw XML, so it passes that guard untouched while
/// still driving `usvg`'s real resolution — and, on this codebase's 256KB small-stack test probe, the whole
/// process — to the same recursion depth as if it *were* nested that deep.
///
/// One unified cap over the union of all six edge types (rather than a separate cap per type) is
/// deliberate: it's simpler to reason about and test, and a mixed chain (e.g. a `<use>` into a `<clipPath>`
/// into a `<mask>`) shouldn't get MORE budget just because it changes reference type partway through — the
/// real recursion cost that matters is the total hop count, not which attribute each hop used.
///
/// 128 is unchanged from CPE-1437 attempt 1's `<use>`-only cap (sized the same way
/// [`MAX_SVG_NESTING_DEPTH`]'s 64 was): a real hand-authored or tool-exported SVG's reference indirection —
/// `<use>` hops, clip-path/mask layering, pattern/marker nesting — is essentially always 1-3 hops (icon
/// sprite sheets and a couple of layered effects are the deepest realistic case, rarely past single digits),
/// so 128 costs nothing for legitimate artwork. It's now checked against attempt 3's audit findings too:
/// ~40-60x below the empirical clipPath (N=8000) and mask (N=5000) `STATUS_STACK_OVERFLOW` floors — which,
/// per the module doc comment, aren't bounded by `usvg`'s own 1024 cap at all, so this pre-scan is the ONLY
/// thing standing between those floors and the caller — and still comfortably below `usvg`'s internal
/// `<use>`-resolution recursion cap of 1024, which is sized for `usvg`'s *own* DoS protection on a
/// normal-sized stack, not for this codebase's small-stack safety bar.
const MAX_REFERENCE_CHAIN_DEPTH: usize = 128;

/// The deepest **combined (product) cost** [`reference_chain_too_deep`] allows before rejecting the
/// document (CPE-1444 — the second dimension the hop-only [`MAX_REFERENCE_CHAIN_DEPTH`] cap was blind to,
/// closing the last parked CPE-1437 vector). See the module doc comment's "attempt 4 / CPE-1444" section:
/// for the MULTIPLICATIVE reference types (`mask`, `filter`/`feImage`, `fill`/`stroke`-referenced
/// `pattern`, `marker-*`), `usvg` descends each hop target's OWN literal `<g>`/element nesting *while the
/// reference-chain recursion is still on the stack*, so its real native recursion depth is not the hop
/// count alone (bounded by [`MAX_REFERENCE_CHAIN_DEPTH`]) nor a single document's own nesting alone
/// (bounded by [`MAX_SVG_NESTING_DEPTH`]) but roughly their **product** — `Σ over the chain of each
/// multiplicative hop target's max nesting depth`. A chain of 127 mask/pattern/marker hops where each hop's
/// `url(#…)` sits at the bottom of 62 nested `<g>` levels passes BOTH independent caps (127 < 128 hops,
/// each document ≤ 64 nesting) yet drives `usvg` to ≈127×64 ≈ 8100 stack frames — enough to overflow even
/// the [`RASTERIZE_STACK_SIZE`] 16MiB thread (a release-build `STATUS_STACK_OVERFLOW` floor empirically
/// ≈7874 frames; in a debug build — which CI runs — the per-frame cost is higher, so the floor drops to
/// ≈127×35 ≈ 4500 frames). [`reference_chain_too_deep`] therefore accumulates this product cost along the
/// chain (a SECOND dimension added to the same DFS that tracks hop depth) and rejects past this cap.
///
/// 2048 is chosen to sit comfortably UNDER both empirical overflow floors — ≈2.2x below the ~4500 debug
/// floor and ≈3.8x below the ~7874 release floor, so genuinely dangerous input is rejected with wide
/// margin on the profile CI actually builds — while sitting FAR ABOVE any legitimate artwork: a real
/// hand-authored or tool-exported SVG's reference indirection is 1–3 hops and its per-hop group nesting a
/// handful of levels, so even a generous "5 hops × 12-deep groups" illustration costs ~60, ~34x under this
/// cap. `clip-path` is deliberately EXCLUDED from this product (it stays additive — `usvg` resolves a clip
/// chain separately from group descent, so its cost is the hop count alone, still bounded by
/// [`MAX_REFERENCE_CHAIN_DEPTH`]), and `<use>` cloning is likewise excluded (bounded by `usvg`'s own
/// ~1,000,000-node / 1024-`<use>`-depth caps and the 16MiB stack — the composition class CPE-1437 attempt 2
/// already closed). Both still count toward the hop cap, they just add nothing to the product cost.
const MAX_REFERENCE_COMBINED_COST: usize = 2048;

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

/// Attributes whose value is a `url(#id)` reference into another element (as opposed to `<use>`'s bare-IRI
/// `href`/`xlink:href` — see [`resolve_use_href`]), enumerated from reading `usvg-0.45.1`'s own converter
/// source (`parser/clippath.rs`, `parser/mask.rs`, `parser/filter.rs`, `parser/paint_server.rs`,
/// `parser/marker.rs` — see the module doc comment's "attempt 3" section for what each one recurses into
/// and why none of them is bounded by `usvg`'s 1024-level `<use>`-resolution cap). `fill`/`stroke` only
/// matter when their `url(#id)` target is a `<pattern>` (a plain color or gradient reference is a
/// self-contained leaf either way — see [`hops_from_target`]), but including them unconditionally here is
/// harmless: a resolved id that turns out not to be pattern-shaped just becomes a dead end in the graph.
const FUNC_IRI_ATTRS: &[&str] =
    &["clip-path", "mask", "filter", "marker-start", "marker-mid", "marker-end", "fill", "stroke"];

/// Whether a [`FUNC_IRI_ATTRS`] reference is **multiplicative** — i.e. resolving it makes `usvg` descend
/// the target's own literal `<g>`/element nesting *on the same call stack* as the reference-chain recursion
/// (CPE-1444 — see [`MAX_REFERENCE_COMBINED_COST`] and the module doc comment's "attempt 4" section). Every
/// func-IRI attr here is multiplicative EXCEPT `clip-path`: `usvg` resolves a clip chain separately from
/// group descent (`clippath::convert` recurses clip-to-clip, but does NOT descend each clip's inner group
/// nesting while that clip-chain frame is live), so clip cost is the hop count alone — additive, bounded by
/// [`MAX_REFERENCE_CHAIN_DEPTH`], contributing nothing to the product cost. `mask`/`filter`/`marker-*` and
/// `fill`/`stroke` (when they reference a `<pattern>`) all convert their target's content through the same
/// general `converter::convert_children` used for ordinary element children, descending that content's full
/// nesting on-stack — so they ARE multiplicative.
fn func_iri_attr_is_multiplicative(attr: &str) -> bool {
    attr != "clip-path"
}

/// The maximum literal element-nesting depth within `target`'s own subtree (`target` itself = depth 1),
/// memoized in `cache`. This is the depth `usvg` descends *on-stack* when it converts `target`'s subtree
/// while resolving a MULTIPLICATIVE reference to it (see [`func_iri_attr_is_multiplicative`] and
/// [`MAX_REFERENCE_COMBINED_COST`]) — the per-hop cost accumulated along the reference chain. Using the
/// subtree's MAX nesting (rather than the exact depth of the particular reference-bearing descendant a hop
/// lands on) is a deliberate over-approximation: it can only ever make this guard MORE conservative, never
/// less, and it's cheap to compute and cache once per target.
///
/// Walks the subtree with an explicit heap-allocated stack — never the real call stack — so it can't
/// itself overflow no matter how the subtree is shaped. (In practice the whole document already passed
/// [`xml_nesting_too_deep`]'s 64-level literal-nesting cap before this runs, so every subtree depth here is
/// ≤ 64; the explicit-stack walk is defensive belt-and-braces regardless.)
fn subtree_nesting_depth<'a>(target: XmlNode<'a>, cache: &mut HashMap<XmlNode<'a>, usize>) -> usize {
    if let Some(&d) = cache.get(&target) {
        return d;
    }
    let mut max_depth = 0usize;
    let mut stack: Vec<(XmlNode<'a>, usize)> = vec![(target, 1)];
    while let Some((node, d)) = stack.pop() {
        if d > max_depth {
            max_depth = d;
        }
        for child in node.children() {
            if child.is_element() {
                stack.push((child, d + 1));
            }
        }
    }
    cache.insert(target, max_depth);
    max_depth
}

/// Finds every `url(#id)`-shaped reference anywhere in `value`, mirroring `svgtypes::FuncIRI::from_str`'s
/// grammar (`url(` + optional whitespace + optional matching quote + `#id` + optional whitespace + `)`) by
/// hand for the same no-new-dependency reason [`parse_iri_fragment`] mirrors `IRI::from_str` (see that
/// function's doc comment). Deliberately more permissive than `FuncIRI::from_str`'s exact single-value
/// grammar in two ways: it scans for a match ANYWHERE in `value` rather than requiring the whole value to
/// be exactly one reference (needed because `filter` can legally hold a LIST of space-separated filter
/// functions and `url(...)` references, e.g. `filter="blur(2) url(#f1) url(#f2)"`), and it's lenient about
/// quote-pairing. Being more permissive only makes this guard MORE conservative, never less: extra
/// surrounding text that would make `usvg` reject the whole attribute can at worst add an extra graph edge
/// here, never silently drop a real one.
///
/// Purely iterative (a single scan with a small bounded lookahead per `url(` occurrence) — never recurses —
/// so a value packed with many `url(` tokens (valid ids or not) can't overflow any stack or make this scan
/// itself do unbounded work.
fn find_func_iri_ids(value: &str) -> Vec<&str> {
    const LOOKAHEAD: usize = 4096;
    let bytes = value.as_bytes();
    let mut ids = Vec::new();
    let mut pos = 0usize;
    while pos + 4 <= bytes.len() {
        if &bytes[pos..pos + 4] != b"url(" {
            pos += 1;
            continue;
        }
        let paren = pos + 4;
        let limit = bytes.len().min(paren + LOOKAHEAD);
        let mut i = paren;
        while i < limit && bytes[i] != b'#' && bytes[i] != b')' {
            i += 1;
        }
        if i < limit && bytes[i] == b'#' {
            let id_start = i + 1;
            let mut j = id_start;
            while j < bytes.len() && !matches!(bytes[j], b')' | b' ' | b'\'' | b'"') {
                j += 1;
            }
            if j > id_start {
                // `id_start`/`j` only ever land on ASCII delimiters, so this slice is always a valid UTF-8
                // char boundary (same reasoning as `parse_iri_fragment`'s own slicing).
                ids.push(&value[id_start..j]);
            }
            pos = j.max(paren + 1);
        } else {
            pos = paren;
        }
    }
    ids
}

/// The reference-graph edges directly on `node` itself: `<use>`/`<feImage>`'s bare-IRI `href` (resolved via
/// [`resolve_use_href`], reusing the exact `xlink:href`-before-`href` precedence CPE-1414 validated), plus
/// every `url(#id)` found in any of [`FUNC_IRI_ATTRS`]'s attributes (resolved via [`find_func_iri_ids`]).
/// An id that doesn't resolve to any element in `id_map` contributes no edge — matching `usvg`, which
/// treats an unresolvable reference as absent rather than guessing.
///
/// Each returned target carries a `bool` = whether that reference is **multiplicative** (CPE-1444 — its
/// resolution descends the target's own nesting on-stack, so it contributes a per-hop nesting cost to
/// [`MAX_REFERENCE_COMBINED_COST`]; see [`func_iri_attr_is_multiplicative`]). `<use>`'s bare-IRI `href` is
/// additive (bounded by `usvg`'s node/`<use>`-depth caps + the 16MiB stack), but `<feImage>`'s `href`
/// resolves an ARBITRARY element via the same general converter that descends its subtree on-stack — so
/// `feImage` is multiplicative even though it shares `<use>`'s `href` resolution.
fn direct_reference_targets<'a>(
    node: XmlNode<'a>,
    id_map: &HashMap<&'a str, XmlNode<'a>>,
) -> Vec<(XmlNode<'a>, bool)> {
    let mut targets = Vec::new();

    if node.has_tag_name("use") || node.has_tag_name("feImage") {
        if let Some(frag) = resolve_use_href(node) {
            if let Some(&t) = id_map.get(frag) {
                targets.push((t, node.has_tag_name("feImage")));
            }
        }
    }

    for &attr in FUNC_IRI_ATTRS {
        if let Some(value) = node.attribute(attr) {
            for id in find_func_iri_ids(value) {
                if let Some(&t) = id_map.get(id) {
                    targets.push((t, func_iri_attr_is_multiplicative(attr)));
                }
            }
        }
    }

    targets
}

/// The reference-BEARING nodes a resolution of `target` will encounter next — i.e. the nodes themselves,
/// each with its OWN resolution still deferred to when the DFS visits it (via [`chain_edges`] again), NOT
/// their already-resolved targets. This distinction matters: returning an already-resolved target here
/// would silently skip a hop (double-advancing the chain two links at once instead of one), which is
/// exactly the subtle bug an earlier version of this generalization had — caught by the boundary test
/// (`use_chain_guard_boundary_exactly_at_the_cap_is_allowed_one_more_is_rejected` in this module's tests)
/// quietly passing a 129-link chain that should have been rejected, because each level was silently
/// advancing by 2 links instead of 1.
///
/// `usvg` converts `target`'s ENTIRE subtree when resolving a reference to it (cloning it for `<use>`;
/// walking its children in place for `<clipPath>`/`<mask>`/`<pattern>`/`<marker>`/`<filter>` content), so
/// this scans every node in `target`'s subtree (via `target.descendants()`, which — per `roxmltree`'s own
/// docs — includes `target` itself first) and keeps any node that is ITSELF reference-bearing (has at
/// least one of its own [`direct_reference_targets`]) as a further hop, unresolved. This single rule
/// generalizes CPE-1437 attempt 1's `<use>`-only "container -> nested `<use>` descendants" logic (which
/// caught the CPE-1414 mutual-`<symbol>` cycle even though neither `<use>` named the other directly:
/// `target.descendants().filter(|n| n.has_tag_name("use"))`, i.e. exactly this same "find reference-bearing
/// nodes, don't resolve them yet" shape) to ALL six reference types at once: a `<clipPath id="a">`
/// containing another `<clipPath id="b">`'s `clip-path` reference, a `<pattern>` whose content fills a
/// shape with another pattern, a `<filter>` whose `feImage` references an element with its own `filter=`,
/// and so on are all found the same way — including `target` itself being reference-bearing (e.g. `target`
/// is itself a `<use>`, or a `<clipPath>` that itself has `clip-path=`), which is why the scan doesn't skip
/// `target` in its own subtree.
///
/// `target_cache` memoizes this per TARGET node (not per DFS-visited node — see [`reference_chain_too_deep`]
/// for that separate, path-based memoization) so that many different trigger nodes independently referencing
/// the SAME target (a common, entirely legitimate pattern — e.g. many shapes sharing one `fill="url(#p)"`)
/// don't each re-scan that target's whole subtree from scratch; without this, a document with many
/// independent references into one large shared target would make this pre-scan itself do quadratic work.
fn hops_from_target<'a>(
    target: XmlNode<'a>,
    id_map: &HashMap<&'a str, XmlNode<'a>>,
    target_cache: &mut HashMap<XmlNode<'a>, Vec<XmlNode<'a>>>,
) -> Vec<XmlNode<'a>> {
    if let Some(cached) = target_cache.get(&target) {
        return cached.clone();
    }
    let hops: Vec<_> = target
        .descendants()
        .filter(|&n| !direct_reference_targets(n, id_map).is_empty())
        .collect();
    target_cache.insert(target, hops.clone());
    hops
}

/// The full set of reference-chain hops directly reachable from `node` (see [`direct_reference_targets`]
/// for `node`'s own direct targets, then [`hops_from_target`] for what resolving each of those targets
/// leads to next). A direct self-reference (one of `node`'s targets resolves back to `node` itself) is
/// excluded before expanding further — matching `usvg`'s own explicit `link == node` self-reference guards
/// in `clippath::convert`/`mask::convert`/`use_node.rs` (all silently skip rendering rather than recursing)
/// — so this doesn't over-reject the harmless `<use href="#self">`/`<clipPath id="a" clip-path="url(#a)">`
/// idiom as if it were a reference cycle.
///
/// Each edge carries the **per-hop cost** taking it adds to the reference chain's product cost (CPE-1444):
/// the target subtree's own [`subtree_nesting_depth`] for a MULTIPLICATIVE reference (`usvg` descends that
/// nesting on-stack while the chain recursion is live), or `0` for an additive one (`clip-path`, `<use>`).
/// All hops discovered inside one target share that target's cost, since resolving the reference descends
/// that one target's subtree once. [`reference_chain_too_deep`] accumulates these along the chain.
fn chain_edges<'a>(
    node: XmlNode<'a>,
    id_map: &HashMap<&'a str, XmlNode<'a>>,
    target_cache: &mut HashMap<XmlNode<'a>, Vec<XmlNode<'a>>>,
    nesting_cache: &mut HashMap<XmlNode<'a>, usize>,
) -> Vec<ChainEdge<'a>> {
    let mut edges = Vec::new();
    for (target, multiplicative) in direct_reference_targets(node, id_map) {
        if target == node {
            continue;
        }
        let cost = if multiplicative { subtree_nesting_depth(target, nesting_cache) } else { 0 };
        for hop in hops_from_target(target, id_map, target_cache) {
            edges.push((hop, cost));
        }
    }
    edges
}

/// Cheap(ish), non-recursive guard against a maliciously (or accidentally) deep reference chain of ANY of
/// the six kinds enumerated in the module doc comment's "attempt 3" section (`<use>`, `clip-path`, `mask`,
/// `filter`+`feImage`, `fill`/`stroke`-referenced `pattern`, `marker-*`), run before the document is handed
/// to `usvg`/`resvg::render`. See the module doc comment for the full backstory; in short this complements
/// [`xml_nesting_too_deep`] (CPE-1413), which only bounds *literal* XML nesting and is blind to a flat
/// sibling reference chain of any of these six shapes.
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
/// Deliberately does **not** pre-filter on whether the raw bytes contain any particular substring before
/// paying for the full parse: that kind of fast path is itself a potential bypass (an entity/DTD-obfuscated
/// reference might not contain the literal substring pre-decode), and this is a low-frequency,
/// security-sensitive code path (thumbnail generation, not a hot per-frame loop) where correctness is
/// worth more than shaving a parse.
///
/// Walks the reference graph with an explicit heap-allocated stack (never the real call stack, so this
/// scan itself can't overflow no matter how deep or cyclic the input is), doing an iterative post-order DFS
/// from every reference-bearing element in the document (any node [`direct_reference_targets`] resolves at
/// least one edge for — not just `<use>` anymore, since CPE-1437 attempt 3), memoizing each node's resolved
/// chain depth so no node is ever walked twice. A **cycle** (revisiting a node that's still `InProgress` on
/// the current path — this is what also catches the CPE-1414 mutual-`<symbol>` reference cycle, since
/// that's an unbounded chain by construction) is treated as exceeding the cap immediately: either way
/// `usvg` would recurse without bound (a true cycle) or well past this codebase's small-stack safety margin
/// (a merely very long chain), so both are rejected identically here rather than trying to special-case
/// "genuinely infinite" vs. "just very deep". Also bails out the moment the *currently open* DFS path alone
/// exceeds `max_depth` (before exploring any further), so a single pathological chain can't force this scan
/// to do more than `O(max_depth)` work before rejecting it.
fn reference_chain_too_deep(bytes: &[u8], max_depth: usize, max_cost: usize) -> bool {
    // In the `rasterize_svg` path this branch is normally unreachable: `rasterize_svg` already
    // gzip-decompresses SVGZ input once, bounded, before this function ever runs, AND (CPE-1445 attempt 2)
    // rejects outright if that single decompress still leaves the `1F 8B` magic (a doubly-gzipped
    // payload) — so `bytes` here can never carry the gzip magic on that path, single- or multi-layer
    // alike. Kept as defense-in-depth for any future direct caller, using the SAME bounded decompressor
    // `rasterize_svg` uses (rather than `usvg`'s own uncapped `decompress_svgz`) so this function can
    // never itself become an unbounded-decompression path — and if handed a DOUBLY-gzipped stream
    // directly, the one decompress here still leaves the result gzip-magic'd, which fails the subsequent
    // UTF-8 check below (gzip's binary bytes are essentially never valid UTF-8) and returns `false`
    // gracefully; this function never hands anything to `usvg`, so unlike `rasterize_svg` it has no
    // OOM/overflow exposure from that residual case, only a (harmless) missed pre-scan rejection.
    let decompressed;
    let text: &str = if bytes.starts_with(&[0x1f, 0x8b]) {
        decompressed = match decompress_svgz_bounded(bytes) {
            Ok(d) => d,
            Err(_) => return false, // malformed/oversized gzip: usvg's real parse will report this itself
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

    let mut target_cache: HashMap<XmlNode, Vec<XmlNode>> = HashMap::new();
    let mut nesting_cache: HashMap<XmlNode, usize> = HashMap::new();
    let trigger_nodes: Vec<_> = doc
        .descendants()
        .filter(|&n| !direct_reference_targets(n, &id_map).is_empty())
        .collect();
    if trigger_nodes.is_empty() {
        return false;
    }

    // Each resolved node memoizes BOTH dimensions of the worst (most expensive) chain starting at it:
    // `depth` = hop count (bounded by `max_depth`, CPE-1437), and `cost` = accumulated multiplicative
    // per-hop nesting = `Σ over the chain of each multiplicative hop target's own nesting depth` (bounded
    // by `max_cost`, CPE-1444). Both are path-independent properties of the node's outgoing subgraph, so
    // memoizing per node (a node reachable by many chains keeps its worst value) is correct — see
    // [`MAX_REFERENCE_COMBINED_COST`].
    enum VisitState {
        InProgress,
        Done { depth: usize, cost: usize },
    }
    let mut state: HashMap<XmlNode, VisitState> = HashMap::new();

    for &start in &trigger_nodes {
        if state.contains_key(&start) {
            continue; // already resolved as part of an earlier start node's walk
        }

        // Explicit heap-allocated stack, non-recursive DFS: each frame is (node, its outgoing (edge,
        // per-hop cost) pairs, index of the next edge to explore).
        let mut stack: Vec<DfsFrame> = Vec::new();
        state.insert(start, VisitState::InProgress);
        stack.push((start, chain_edges(start, &id_map, &mut target_cache, &mut nesting_cache), 0));

        while !stack.is_empty() {
            let top = stack.len() - 1;
            let idx = stack[top].2;
            if idx < stack[top].1.len() {
                let (next, _edge_cost) = stack[top].1[idx];
                stack[top].2 += 1;
                match state.get(&next) {
                    Some(VisitState::Done { .. }) => {} // already resolved; used when this frame unwinds
                    Some(VisitState::InProgress) => return true, // cycle -> unbounded chain -> reject
                    None => {
                        state.insert(next, VisitState::InProgress);
                        let edges = chain_edges(next, &id_map, &mut target_cache, &mut nesting_cache);
                        stack.push((next, edges, 0));
                        if stack.len() > max_depth {
                            // The currently-open DFS path is already longer than the hop cap. Whatever
                            // depth the deepest node on it ends up with (computed on unwind below) can only
                            // be >= this path length, so this can reject right now without exploring
                            // further. (The `cost` dimension can't early-reject this cheaply — per-hop cost
                            // varies — but since every hop cost is bounded by `MAX_SVG_NESTING_DEPTH` and
                            // the open path is bounded here by `max_depth`, the post-order cost check below
                            // still fires within O(max_depth) work.)
                            return true;
                        }
                    }
                }
            } else {
                // All of `node`'s edges are resolved (`Done`) — compute and memoize BOTH its own chain
                // depth (1 + deepest child) and its own worst accumulated cost (max over edges of
                // `this edge's per-hop cost + that child's own accumulated cost`).
                let (node, edges, _) = stack.pop().unwrap();
                let mut max_child_depth = 0usize;
                let mut max_path_cost = 0usize;
                for &(child, edge_cost) in &edges {
                    let (child_depth, child_cost) = match state.get(&child) {
                        Some(VisitState::Done { depth, cost }) => (*depth, *cost),
                        _ => (0, 0),
                    };
                    if child_depth > max_child_depth {
                        max_child_depth = child_depth;
                    }
                    let path_cost = edge_cost.saturating_add(child_cost);
                    if path_cost > max_path_cost {
                        max_path_cost = path_cost;
                    }
                }
                let depth = 1 + max_child_depth;
                if depth > max_depth || max_path_cost > max_cost {
                    return true;
                }
                state.insert(node, VisitState::Done { depth, cost: max_path_cost });
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
/// that puts the real per-level stack cost at a few KB; 16MiB is 64x that probe's total size. That is
/// enough headroom for the ONE class this stack is responsible for — the `<use>`/nesting *composition*
/// class, whose depth `usvg` itself hard-caps at 1024 `<use>`-resolution levels regardless of caller stack
/// size — but it is explicitly **NOT** "several thousand levels comfortably past everything": CPE-1437
/// attempt 3 empirically found `usvg`'s `clip-path`/`mask`/`filter`/`pattern`/`marker` reference recursions
/// are bounded only by total element count (~1,000,000), not by that 1024 cap, so a long enough acyclic
/// chain of them overflows even this 16MiB stack (confirmed floors N≈5000–8000), and CPE-1444 found the
/// MULTIPLICATIVE subset (`mask`/`filter`/`pattern`/`marker`) overflows it at a mere ≈7874 frames
/// (≈127 hops × 62 nesting). Those are the province of [`reference_chain_too_deep`]'s hop cap
/// ([`MAX_REFERENCE_CHAIN_DEPTH`]) and combined-cost cap ([`MAX_REFERENCE_COMBINED_COST`]), NOT of this
/// stack — `clip-path` is additive/hop-capped and `<use>` is node-capped, while `mask`/`filter`/`pattern`/
/// `marker` need the product bound. This stack is empirically confirmed safe against every payload in
/// `tests/thumb_svg_panic_safety.rs` that is legitimately within `usvg`'s own bounds
/// (composition-of-hops-and-nesting, plain deep nesting, the mutual `<symbol>` cycle, and shallow chains);
/// the input-unbounded chains are rejected by the pre-scan before they ever reach this thread.
const RASTERIZE_STACK_SIZE: usize = 16 * 1024 * 1024;

/// Rasterize `bytes` (the contents of an `.svg` file) to an RGBA image whose longest edge is at most
/// `max_edge` pixels, preserving the document's aspect ratio. Never panics — and, critically, never
/// **stack-overflows the calling thread**, which is a stronger guarantee than "never panics" since a
/// stack overflow is uncatchable and aborts the whole process regardless of `catch_unwind`: see
/// [`rasterize_svg_on_a_guaranteed_stack`] for how. A malformed document, an implausible declared size, or
/// a render-target allocation failure all return `Err`.
///
/// [`xml_nesting_too_deep`] and [`reference_chain_too_deep`] still run first as cheap, non-recursive
/// fast-reject checks — genuinely useful defense-in-depth against obviously pathological input (an
/// absurdly deep document or an absurdly long/cyclic reference chain costs real CPU/memory even with a
/// big enough stack to survive it) — but per CPE-1437 attempts 2 and 3 they are **not sufficient as the
/// stack-overflow safety bar on their own**. See the module doc comment for the full story:
/// - Attempt 2: two depth-prediction guards (CPE-1414's cycle-guard attempts, then this file's own
///   CPE-1437 attempt 1) were each defeated by an adversarial reviewer finding an input shape the guard's
///   model didn't account for (a payload composing `<use>`-hop count with each hop's target's internal
///   `<g>`-nesting depth, and a pre-existing CPE-1413 hole where even plain nesting under its own cap
///   overflowed `usvg`'s tree-*conversion* pass). Trying to extend a depth-prediction guard with a third
///   dimension only invites a fourth bypass, so [`rasterize_svg_on_a_guaranteed_stack`]'s dedicated
///   large-stack thread became the primary defense for anything `usvg` itself caps at 1024 levels.
/// - Attempt 3: a large stack alone does NOT close everything — `usvg` has several OTHER
///   reference-resolution recursions (`clip-path`, `mask`, `filter`+`feImage`, `pattern`-via-`fill`/
///   `stroke`, `marker-*`) bounded only by total element count (~1,000,000), not by that 1024 cap, so an
///   acyclic chain of them overflows even a 16MiB stack (confirmed reproducers at N=5000-8000). No stack
///   size closes an input-unbounded recursion — so [`reference_chain_too_deep`] (generalized from
///   `<use>`-only to all six reference types) is back to being load-bearing for THOSE six, while the
///   guaranteed stack remains the defense for the `<use>`/nesting composition class attempt 2 fixed.
pub fn rasterize_svg(bytes: &[u8], max_edge: u32) -> Result<DynamicImage, String> {
    // SVGZ (gzip-compressed .svg): decompress ONCE, up front, bounded (CPE-1445 — see the module doc
    // comment), before ANY guard below runs. This must happen first and only once: `xml_nesting_too_deep`
    // is a pure byte scan with no gzip awareness at all, so handing it the still-compressed bytes would
    // silently bypass it (it'd see no '<' tags and report "not too deep" regardless of what the
    // decompressed XML actually looks like); and decompressing unboundedly — as `usvg::Tree::from_data`'s
    // own internal gzip handling does — lets a tiny crafted stream force a multi-gigabyte allocation. By
    // decompressing here and threading the DECOMPRESSED bytes through everything that follows, both the
    // nesting guard and the real `usvg` parse (inside `rasterize_svg_on_a_guaranteed_stack`, which no
    // longer sees the gzip magic and so never re-decompresses) operate on the same bounded, plain-XML
    // bytes exactly once.
    let bytes: Vec<u8> = if bytes.starts_with(&[0x1f, 0x8b]) {
        let decompressed = decompress_svgz_bounded(bytes)?;
        // CPE-1445 attempt 2 (adversarial re-audit): a DOUBLY-gzipped `.svg` (gzip of a gzip of an SVG)
        // decompresses ONE layer here and still starts with `1F 8B` — the inner gzip stream. A single
        // bounded decompress does not, on its own, establish "the bytes handed onward are plain XML";
        // without this check those still-compressed bytes would flow to `usvg::Tree::from_data` below,
        // which re-detects the magic and decompresses the inner layer itself, UNBOUNDED — reopening both
        // CPE-1445 sub-bugs one gzip layer down (a doubly-gzipped deep-nesting payload would reach
        // `usvg`/`roxmltree` without ever passing through `xml_nesting_too_deep` on real XML, and a
        // doubly-gzipped bomb would let the inner layer's uncapped `read_to_end` run past this cap). A
        // legitimate SVGZ file is always exactly ONE gzip layer (that's what every SVG authoring/export
        // tool and the SVGZ format itself produce), so nested gzip has no legitimate use here — reject it
        // outright rather than looping the bounded decompress across layers, which would only move the
        // "how many layers is too many" question to a new, equally-arbitrary cap.
        if decompressed.starts_with(&[0x1f, 0x8b]) {
            return Err("nested (double-gzipped) SVG rejected".to_string());
        }
        decompressed
    } else {
        bytes.to_vec()
    };

    // Only the pure byte-scan guard runs on the CALLER's own thread — it's provably non-recursive (a flat
    // loop with an integer counter, see its own doc comment), so it can't overflow any stack regardless of
    // input depth, and rejecting an obviously-pathological document this cheaply avoids paying for a
    // dedicated large-stack thread spawn at all in the common "somebody sent garbage" case.
    if xml_nesting_too_deep(&bytes, MAX_SVG_NESTING_DEPTH) {
        return Err("SVG element nesting too deep".to_string());
    }

    // Everything else — including `reference_chain_too_deep` — runs inside the guaranteed-large-stack
    // thread below. This is deliberate, not an oversight: `reference_chain_too_deep` itself calls the
    // REAL `roxmltree::Document::parse_with_options` (needed to mirror `usvg`'s own entity/DTD decoding —
    // see that function's doc comment), which is exactly the same per-nesting-level-recursive parser
    // `xml_nesting_too_deep` exists to keep away from a small caller stack in the first place. A CPE-1437
    // attempt 2 diagnostic (see the module doc comment) confirmed a BARE `roxmltree` parse of a document
    // only ~42 levels deep — comfortably under `xml_nesting_too_deep`'s own 64-level cap — already
    // overflows a 256KB stack by itself, so leaving that parse on the caller's thread would have silently
    // reintroduced the exact bug class this whole attempt exists to close.
    rasterize_svg_on_a_guaranteed_stack(bytes, max_edge)
}

/// Does the actual reference-chain check *and* the `usvg` parse + convert + `resvg` render on a
/// **dedicated thread** with [`RASTERIZE_STACK_SIZE`] of stack, regardless of the calling thread's own
/// stack size (which, in production, is already the Tokio `spawn_blocking` default of 2MiB — plenty on
/// its own, but this function no longer wants to depend on that; a caller on a smaller stack, like this
/// crate's own small-stack panic-safety probe, must be just as safe). `rasterize_svg` now **owns** its
/// stack requirement instead of inheriting the caller's, so no `<use>`/nesting-composition input `usvg`
/// itself is willing to accept — bounded by its own hard cap of 1024 `<use>`-resolution recursion levels
/// — can overflow it (CPE-1437 attempt 2). This does NOT, on its own, bound the five other
/// reference-resolution recursions attempt 3 found (see the module doc comment) — those are only
/// input-count-limited by `usvg`, not depth-limited, so [`reference_chain_too_deep`] below is what closes
/// them, on this same guaranteed stack.
///
/// [`reference_chain_too_deep`] runs FIRST inside this closure (still cheap relative to a full render, and
/// still valuable as defense-in-depth against a pathologically long/cyclic reference chain of any of the
/// six kinds it now covers), but now on this function's own guaranteed stack rather than the caller's —
/// see [`rasterize_svg`]'s doc comment for why that placement specifically matters.
///
/// A panic inside the closure (e.g. an allocation failure surfaced as a panic rather than an `Err`) is
/// caught via the thread's own unwind boundary and turned into a graceful `Err` through `JoinHandle::join`
/// — only a genuine stack *overflow* is uncatchable, and that's exactly the failure mode this function
/// exists to make unreachable. Failing to spawn the thread at all (OS resource exhaustion) is itself just
/// another `Err`, never a panic.
fn rasterize_svg_on_a_guaranteed_stack(bytes: Vec<u8>, max_edge: u32) -> Result<DynamicImage, String> {
    let render = move || -> Result<DynamicImage, String> {
        if reference_chain_too_deep(&bytes, MAX_REFERENCE_CHAIN_DEPTH, MAX_REFERENCE_COMBINED_COST) {
            return Err("SVG reference chain too deep".to_string());
        }

        // Defense-in-depth (CPE-1445 attempt 2): `bytes` here must NEVER carry the gzip magic — by this
        // point `rasterize_svg` has already decompressed any single gzip layer (bounded) and rejected a
        // still-gzip-magic'd result outright (a doubly-gzipped payload). This is a graceful-`Err` belt
        // enforcing that invariant right at the boundary into `usvg`, so a future change upstream in this
        // function (or a new caller of `rasterize_svg_on_a_guaranteed_stack`) can't silently reintroduce
        // "usvg re-decompresses an unbounded inner gzip layer" even by accident — `usvg::Tree::from_data`
        // itself has no size cap on ITS OWN internal `decompress_svgz` fallback, so it must never be
        // handed bytes it could interpret as gzip.
        if bytes.starts_with(&[0x1f, 0x8b]) {
            return Err(
                "internal invariant violation: gzip-magic bytes reached the usvg parse boundary \
                 (should have been rejected earlier in rasterize_svg)"
                    .to_string(),
            );
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
        assert!(reference_chain_too_deep(&flat_use_chain_svg(500), MAX_REFERENCE_CHAIN_DEPTH, MAX_REFERENCE_COMBINED_COST));
        // A realistic sprite-sheet-style chain (a couple of hops) must not be rejected.
        assert!(!reference_chain_too_deep(&flat_use_chain_svg(3), MAX_REFERENCE_CHAIN_DEPTH, MAX_REFERENCE_COMBINED_COST));
        assert!(!reference_chain_too_deep(&square_svg(10, 10), MAX_REFERENCE_CHAIN_DEPTH, MAX_REFERENCE_COMBINED_COST), "no <use> at all");
    }

    #[test]
    fn use_chain_guard_boundary_exactly_at_the_cap_is_allowed_one_more_is_rejected() {
        // A chain of exactly MAX_REFERENCE_CHAIN_DEPTH links resolves to chain depth == the cap, which the
        // guard's own `depth > max_depth` check must allow; one link more must tip it over.
        assert!(
            !reference_chain_too_deep(&flat_use_chain_svg(MAX_REFERENCE_CHAIN_DEPTH), MAX_REFERENCE_CHAIN_DEPTH, MAX_REFERENCE_COMBINED_COST),
            "a chain exactly at the cap must be allowed"
        );
        assert!(
            reference_chain_too_deep(&flat_use_chain_svg(MAX_REFERENCE_CHAIN_DEPTH + 1), MAX_REFERENCE_CHAIN_DEPTH, MAX_REFERENCE_COMBINED_COST),
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
        assert!(reference_chain_too_deep(svg, MAX_REFERENCE_CHAIN_DEPTH, MAX_REFERENCE_COMBINED_COST));
    }

    #[test]
    fn use_chain_guard_allows_a_direct_self_reference() {
        // `<use href="#self">` — usvg treats this as a harmless no-op (link == node), not a cycle; the
        // guard must not over-reject this common-enough idiom.
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <use id="self" href="#self"/>
        </svg>"##;
        assert!(!reference_chain_too_deep(svg, MAX_REFERENCE_CHAIN_DEPTH, MAX_REFERENCE_COMBINED_COST));
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
        assert!(!reference_chain_too_deep(svg, MAX_REFERENCE_CHAIN_DEPTH, MAX_REFERENCE_COMBINED_COST), "a single resolved hop is well under the real cap");
        assert!(
            reference_chain_too_deep(svg, 0, MAX_REFERENCE_COMBINED_COST),
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
            reference_chain_too_deep(s.as_bytes(), MAX_REFERENCE_CHAIN_DEPTH, MAX_REFERENCE_COMBINED_COST),
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

    // -----------------------------------------------------------------------------------------
    // CPE-1444: combined (product) cost — hops × per-hop nesting — for the multiplicative types.
    // -----------------------------------------------------------------------------------------

    /// A chain of `hops` `<mask>`s where mask `m{i}`'s `mask="url(#m{i-1})"` reference sits at the bottom
    /// of `nest` nested `<g>` levels, triggered by an outer `<rect mask="url(#m{hops})">`. The inner
    /// reference-bearing shape is SELF-CLOSING, so total literal nesting is `svg`+`mask`+`nest` — exactly
    /// 64 at `nest`=62, which PASSES [`xml_nesting_too_deep`]'s cap — while `usvg` still descends each hop's
    /// `nest` `<g>` levels on-stack during mask resolution, so the real recursion cost is ≈ `hops × nest`
    /// (the CPE-1444 multiplicative vector). Per-hop [`subtree_nesting_depth`] here is `nest`+2 (mask + the
    /// `nest` `<g>`s + the self-closing rect).
    fn nested_mask_chain(hops: usize, nest: usize) -> Vec<u8> {
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

    /// The `clip-path` analogue of [`nested_mask_chain`] — identical shape, but `clip-path` is ADDITIVE
    /// (usvg resolves the clip chain separately from group descent), so it must NOT be product-costed.
    fn nested_clip_chain(hops: usize, nest: usize) -> Vec<u8> {
        let mut s = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"#);
        s.push_str(r##"<clipPath id="c0"><rect width="10" height="10"/></clipPath>"##);
        for i in 1..=hops {
            s.push_str(&format!(r#"<clipPath id="c{i}">"#));
            s.push_str(&"<g>".repeat(nest));
            s.push_str(&format!(
                r##"<rect width="10" height="10" clip-path="url(#c{prev})"/>"##,
                prev = i - 1
            ));
            s.push_str(&"</g>".repeat(nest));
            s.push_str("</clipPath>");
        }
        s.push_str(&format!(r##"<rect width="10" height="10" clip-path="url(#c{hops})"/>"##));
        s.push_str("</svg>");
        s.into_bytes()
    }

    #[test]
    fn combined_cost_guard_rejects_a_multiplicative_chain_that_passes_the_hop_cap() {
        // The exact CPE-1444 vector: 127 mask hops (== 128 chain nodes, which the hop cap of 128 ALLOWS)
        // each 62 `<g>` deep → combined cost ≈ 127×64 ≫ 2048. Must be rejected by the COST dimension, not
        // the hop dimension — proving the second dimension is load-bearing.
        assert!(reference_chain_too_deep(
            &nested_mask_chain(127, 62),
            MAX_REFERENCE_CHAIN_DEPTH,
            MAX_REFERENCE_COMBINED_COST
        ));
        // And it is specifically the COST cap doing it: with the cost cap raised out of the way but the hop
        // cap unchanged, 127 hops sits at the cap and is allowed — so nothing but the product bound rejects
        // the real payload.
        assert!(!reference_chain_too_deep(&nested_mask_chain(127, 62), MAX_REFERENCE_CHAIN_DEPTH, usize::MAX));
    }

    #[test]
    fn combined_cost_guard_boundary_just_under_is_allowed_just_over_is_rejected() {
        // Per-hop cost is `nest`+2 = 64 at nest=62, so the cap of 2048 is reached at exactly 32 hops
        // (32×64 = 2048, allowed by the `> max_cost` check) and exceeded at 33 (33×64 = 2112).
        assert!(!reference_chain_too_deep(
            &nested_mask_chain(32, 62),
            MAX_REFERENCE_CHAIN_DEPTH,
            MAX_REFERENCE_COMBINED_COST
        ));
        assert!(reference_chain_too_deep(
            &nested_mask_chain(33, 62),
            MAX_REFERENCE_CHAIN_DEPTH,
            MAX_REFERENCE_COMBINED_COST
        ));
    }

    #[test]
    fn combined_cost_guard_does_not_product_cost_additive_clip_path() {
        // `clip-path` is additive: usvg resolves the clip chain separately from group descent, so a 127-hop
        // clip chain each 62 `<g>` deep is bounded by the HOP cap alone (127 < 128 → allowed), NOT the
        // product cap — confirm the guard doesn't over-reject it as if clip were multiplicative.
        assert!(!reference_chain_too_deep(
            &nested_clip_chain(127, 62),
            MAX_REFERENCE_CHAIN_DEPTH,
            MAX_REFERENCE_COMBINED_COST
        ));
        // The hop cap still catches a genuinely too-long clip chain (regression for clip staying capped).
        assert!(reference_chain_too_deep(
            &nested_clip_chain(200, 2),
            MAX_REFERENCE_CHAIN_DEPTH,
            MAX_REFERENCE_COMBINED_COST
        ));
    }

    #[test]
    fn combined_cost_guard_allows_a_legit_shallow_multiplicative_chain() {
        // A realistic layered-effect SVG: a handful of mask hops, a few groups deep — must render, never
        // rejected by the product cap.
        assert!(!reference_chain_too_deep(
            &nested_mask_chain(4, 8),
            MAX_REFERENCE_CHAIN_DEPTH,
            MAX_REFERENCE_COMBINED_COST
        ));
    }

    // -----------------------------------------------------------------------------------------
    // CPE-1445: bounded SVGZ (gzip) decompression. See `tests/thumb_svg_panic_safety.rs` for the
    // full `rasterize_svg`-level (nesting-bypass + gzip-bomb + legit-SVGZ) regression tests on the
    // small-stack probe; these are fast unit-level checks of `decompress_svgz_bounded` itself.
    // -----------------------------------------------------------------------------------------

    fn gzip_bytes(content: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut out = Vec::new();
        let mut enc = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
        enc.write_all(content).unwrap();
        enc.finish().unwrap();
        out
    }

    #[test]
    fn decompress_svgz_bounded_round_trips_a_small_payload() {
        let original = square_svg(10, 10);
        let gz = gzip_bytes(&original);
        let decompressed = decompress_svgz_bounded(&gz).expect("small gzip payload must decompress");
        assert_eq!(decompressed, original);
    }

    #[test]
    fn decompress_svgz_bounded_rejects_malformed_gzip_gracefully() {
        // Bytes starting with the gzip magic but not actually valid gzip past that point.
        let err = decompress_svgz_bounded(&[0x1f, 0x8b, 0xff, 0xff, 0xff]);
        assert!(err.is_err(), "malformed gzip data must be a graceful Err, not a panic");
    }

    #[test]
    fn decompress_svgz_bounded_allows_exactly_the_cap_and_rejects_one_byte_over() {
        // A payload that decompresses to EXACTLY MAX_DECOMPRESSED_SVG_BYTES must be allowed (the cap is a
        // "may reach", not a strict "must stay under"), while one byte more must be rejected — confirms
        // the `+ 1` in the `Read::take` bound and the `> max` (not `>=`) check are both exactly right, not
        // off-by-one in either direction.
        let at_cap = vec![b'A'; MAX_DECOMPRESSED_SVG_BYTES as usize];
        let gz_at_cap = gzip_bytes(&at_cap);
        let decompressed = decompress_svgz_bounded(&gz_at_cap).expect("exactly-at-cap payload must be allowed");
        assert_eq!(decompressed.len() as u64, MAX_DECOMPRESSED_SVG_BYTES);

        let over_cap = vec![b'A'; MAX_DECOMPRESSED_SVG_BYTES as usize + 1];
        let gz_over_cap = gzip_bytes(&over_cap);
        let err = decompress_svgz_bounded(&gz_over_cap);
        assert!(err.is_err(), "one byte past the cap must be rejected");
    }

    #[test]
    fn decompress_svgz_bounded_stops_a_gzip_bomb_without_allocating_the_full_size() {
        // A highly-compressible stream whose true decompressed size is far past the cap (100 MiB of
        // zeros, vs. a 32 MiB cap) — the classic gzip-bomb shape. Must be rejected, and the `Read::take`
        // bound means this can never internally buffer more than `MAX_DECOMPRESSED_SVG_BYTES + 1` bytes
        // regardless of how large the stream's true logical payload is, so this is safe to assert even
        // with a payload well past the cap.
        const CHUNK: usize = 1024 * 1024;
        const LOGICAL_SIZE: usize = 100 * 1024 * 1024;
        let zeros = vec![0u8; CHUNK];
        let mut out = Vec::new();
        {
            use std::io::Write;
            let mut enc = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
            let mut written = 0usize;
            while written < LOGICAL_SIZE {
                enc.write_all(&zeros).unwrap();
                written += CHUNK;
            }
            enc.finish().unwrap();
        }
        assert!(out.len() < LOGICAL_SIZE / 1000, "fixture must actually be a real bomb (high compression ratio)");
        let err = decompress_svgz_bounded(&out);
        assert!(err.is_err(), "a gzip stream past the decompressed-size cap must be rejected, not OOM");
    }

    #[test]
    fn xml_nesting_guard_bypass_is_closed_by_pre_decompressing_gzip_before_the_scan() {
        // CPE-1445's core bug: `xml_nesting_too_deep` itself is (deliberately) a pure byte scan with no
        // gzip awareness at all — handed the RAW gzip bytes directly, it must NOT see the deep nesting
        // (proving the bypass was real and this guard alone can't be blamed/expected to close it). The fix
        // lives in `rasterize_svg`, which now decompresses BEFORE calling this guard — confirmed by the
        // companion `rasterize_svg` test using the same fixture instead of calling this guard directly.
        let gz = gzip_bytes(&deeply_nested_svg(4000));
        assert!(
            !xml_nesting_too_deep(&gz, MAX_SVG_NESTING_DEPTH),
            "the raw byte-scan guard has no gzip awareness by design — it must not see nesting in \
             compressed bytes; rasterize_svg is what pre-decompresses before this guard ever runs"
        );
        // ...but decompressed, the exact same document IS correctly flagged as too deep.
        let decompressed = decompress_svgz_bounded(&gz).unwrap();
        assert!(xml_nesting_too_deep(&decompressed, MAX_SVG_NESTING_DEPTH));
    }

    #[test]
    fn rasterize_svg_rejects_a_gzipped_deeply_nested_svg() {
        // The end-to-end regression: `rasterize_svg` (unlike the bare guard above) must reject this,
        // because it now decompresses SVGZ input up front before running any guard.
        let gz = gzip_bytes(&deeply_nested_svg(4000));
        let err = rasterize_svg(&gz, 32);
        assert!(err.is_err(), "a gzipped implausibly-deep SVG must be rejected via rasterize_svg");
    }

    #[test]
    fn rasterize_svg_renders_a_legit_gzipped_svg() {
        let gz = gzip_bytes(&square_svg(100, 40));
        let img = rasterize_svg(&gz, 32).expect("a legitimate small SVGZ file must still render");
        assert_eq!(img.width(), 32, "longest edge scaled to max_edge, same as the uncompressed case");
    }

    // -----------------------------------------------------------------------------------------
    // CPE-1445 attempt 2: DOUBLY (or N-fold) gzipped `.svg` input. A single bounded decompress alone
    // doesn't guarantee "no gzip magic reaches usvg" — see `tests/thumb_svg_panic_safety.rs` for the
    // small-stack/gzip-bomb-scale regression tests; these are the fast unit-level checks that the
    // outright-reject guard fires exactly on multi-layer input and not on single-layer or plain input.
    // -----------------------------------------------------------------------------------------

    #[test]
    fn rasterize_svg_rejects_a_doubly_gzipped_svg_even_when_the_inner_document_is_tiny_and_legit() {
        // Not a deep-nesting/bomb payload at all — just proves the double-gzip REJECTION itself fires
        // regardless of what's inside the inner layer, since nested gzip has no legitimate SVGZ use case.
        let once = gzip_bytes(&square_svg(10, 10));
        let twice = gzip_bytes(&once);
        assert!(twice.starts_with(&[0x1f, 0x8b]), "outer layer must itself be valid gzip");
        let err = rasterize_svg(&twice, 32);
        assert!(err.is_err(), "a doubly-gzipped SVG must be rejected outright, even with a tiny legit inner document");
    }

    #[test]
    fn rasterize_svg_single_gzip_still_renders_after_the_double_gzip_guard() {
        // Regression: the new "reject if still gzip-magic'd after one decompress" check must not
        // over-reject ordinary single-layer SVGZ input.
        let once = gzip_bytes(&square_svg(10, 10));
        let img = rasterize_svg(&once, 32).expect("single-layer SVGZ must still render after the double-gzip guard");
        assert!(img.width() > 0 && img.height() > 0);
    }
}
