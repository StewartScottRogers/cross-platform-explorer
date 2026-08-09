---
id: CPE-1437
title: "SVG deep ACYCLIC <use> chain overflows the 256KB probe stack (small-stack bar)"
type: Bug
status: Done
priority: Low
component: Backend
tags: [ready]
epic: CPE-718
created: 2026-08-07
---
## Observation (from CPE-1414 adversarial security review, PR #700)
A flat, ACYCLIC chain of ~500 `<use>` elements each referencing the previous (`#u1`←`#u2`←…) passes BOTH
existing guards: `xml_nesting_too_deep` counts element *nesting* (siblings are depth ~1, cap 64) so a flat
chain is shallow, and the CPE-1414 cycle guard correctly does NOT flag it (it's acyclic). usvg resolves
`<use>` by recursive cloning, so resolution depth = chain length; on the 256KB probe stack (`run_on_small_stack`
in `thumb_svg_panic_safety.rs`) a ~500-link chain **STATUS_STACK_OVERFLOWs**.

## Risk = LOW (why it's Deferred, not Backlog)
Production callers rasterize SVGs on a **2MB `spawn_blocking` stack**, and usvg's own recursion cap (1024)
bounds the chain there — so on the real prod stack this does NOT overflow. The overflow only manifests on the
256KB test probe. Same low-risk profile the cycle bug had before CPE-1414 — but it means the *small-stack safety
bar* is not fully closed by a cycle-only guard.

## Fix direction (when picked up)
Bound the `<use>` reference-CHAIN depth (not just nesting + cycles) with a non-recursive pre-scan before usvg —
walk the reference graph and reject if the longest chain exceeds a sane cap (well below usvg's 1024), OR ensure
SVG rasterization always runs on a guaranteed-large stack. Add a deep-acyclic-chain case to the small-stack
battery once bounded.

## Notes
Found by the CPE-1414 adversarial reviewer alongside the (now-fixed) entity-encoded cycle bypass. Not a
regression from CPE-1414 — pre-existing. Track under the thumbnail pipeline epic (CPE-718).

## Work Log
2026-08-07 (sprint worker, branch `cpe-1437-svg-use-chain-depth`) — Implemented in
`crates/server/src/thumb_svg.rs`.

**Important correction to the ticket's own premise:** CPE-1414's "cycle guard" was never actually shipped as
code — that ticket is still `Deferred` with no landed fix (its three PR-#700 attempts each drafted, reviewed,
and reverted; see `Ticketing/Tickets/Deferred/CPE-1414_svg-use-reference-cycle-dos.md`). `thumb_svg.rs` before
this change only had the CPE-1413 literal-nesting guard (`xml_nesting_too_deep`). So there was no existing
walk to "extend" — this ticket builds the walk from scratch, but deliberately reuses CPE-1414's hard-won,
adversarially-validated approach to href resolution (documented in that ticket's Work Log) so the same three
bypass classes can't defeat it.

**The guard (`use_reference_chain_too_deep`, called from `rasterize_svg` before `usvg::Tree::from_data`):**
- Mirrors `usvg`'s own preprocessing exactly: SVGZ-decompresses via `usvg`'s own public `decompress_svgz`
  (reused, not reimplemented), then parses with the identical `roxmltree::ParsingOptions{ allow_dtd: true, .. }`
  `usvg::Tree::from_str` uses — so DTD-internal-subset entities and numeric character references decode
  identically to what `usvg` itself will see.
- `resolve_use_href` mirrors `usvg`'s exact `resolve_href` precedence — `xlink:href` checked FIRST, falling
  back to plain `href` only if absent (not local-name/source-order) — closing CPE-1414 bypass #3.
- `parse_iri_fragment` hand-reimplements `svgtypes::IRI::from_str`'s exact grammar byte-for-byte (`svgtypes`
  is only a transitive dependency via `usvg`, not re-exported, and adding it directly would violate the
  crate's no-new-deps guardrail for a ~20-line parser) — skip XML whitespace, require a leading `#`, take
  bytes to the first literal space or end, require only whitespace after. A value that fails this parse
  (not `#`-prefixed, trailing garbage, etc.) resolves to no edge, exactly like `usvg`.
- The reference graph isn't a naive "does this `<use>` point directly at another `<use>`" chain: if a
  `<use>`'s target is itself a `<use>`, that's one direct hop (the flat-chain shape this ticket reported); if
  the target is a container (`<symbol>`, `<g>`, ...), the walk treats every `<use>` anywhere in that
  container's subtree as a further hop. This dual rule is what lets the SAME walk also catch CPE-1414's
  mutual-`<symbol>`-cycle reproducer (`<symbol id="a">` containing `<use href="#b">`, `<symbol id="b">`
  containing `<use href="#a">`) even though neither `<use>` element names the other directly — the edge from
  `id="a"`'s target to the `<use href="#b">` nested inside it is exactly this kind of hop.
- Walks with an explicit heap-allocated stack (never the real call stack) doing an iterative post-order DFS
  from every `<use>` element, memoizing each node's resolved chain depth (`HashMap<Node, VisitState>` where
  `VisitState` is `InProgress`/`Done(depth)`). A node revisited while still `InProgress` on the current path
  is a cycle — treated as exceeding the cap immediately (an unbounded chain by construction), which is what
  incidentally re-closes CPE-1414's known crash without that ticket's own scope ever being separately shipped.
  Also bails the moment the currently-open DFS path length alone exceeds the cap, so a single pathological
  chain can't force the scan itself to do more than `O(max_depth)` work.

**Cap chosen: `MAX_USE_CHAIN_DEPTH = 128`.** Sized the same way CPE-1413's `MAX_SVG_NESTING_DEPTH` (64) was:
real hand-authored/tool-exported SVGs use `<use>` indirection 1-3 hops deep (icon-sprite sheets referencing a
single base shape are the deepest realistic case), so 128 costs nothing for legitimate artwork; it's
comfortably below the ~500-hop chain confirmed here to `STATUS_STACK_OVERFLOW` a 256KB debug-build thread
stack; and it's comfortably below `usvg`'s own internal recursion cap of 1024 (sized for `usvg`'s own DoS
protection on a normal-size stack, not this codebase's small-stack bar).

**Known, deliberate scope boundary:** the walk only follows `<use>`-to-`<use>` reference-chain edges (directly,
or via a container's nested `<use>` descendants) — it does not attempt to bound a theoretical *combined*
attack that interleaves shallow literal nesting (each individually under the CPE-1413 cap) with `<use>` jumps
into containers holding further deeply-nested-then-jumping content, chained across many separate `<symbol>`s.
That composite shape wasn't found by CPE-1414's adversarial review and is out of this ticket's explicit,
narrower scope ("bound the `<use>` reference-chain depth"); flagged here for whoever next touches this guard.

**Tests:**
- `crates/server/src/thumb_svg.rs` unit tests (14 new): `parse_iri_fragment` grammar (valid/malformed/
  external-URL/empty-fragment cases), `resolve_use_href` xlink-precedence (the CPE-1414 bypass-#3 shape),
  the flat 500-link chain rejected + a 3-link chain allowed, the CPE-1414 mutual-`<symbol>` cycle rejected, a
  direct self-reference (`<use href="#self">`) allowed (matches `usvg`'s own no-op handling, not
  over-rejected), numeric-entity-encoded and DTD-entity-encoded href chains both resolving correctly (bypass
  classes #1/#2), `rasterize_svg` rejecting the deep chain gracefully, and `rasterize_svg` still rendering a
  legitimate 2-`<use>` SVG fine.
- `crates/server/tests/thumb_svg_panic_safety.rs`: added
  `rasterize_svg_never_stack_overflows_on_a_deep_acyclic_use_chain_on_a_small_stack` (the ticket's exact
  500-link reproducer, run on the 256KB probe thread, asserts graceful `Err`) and
  `rasterize_svg_renders_a_shallow_use_chain_fine_on_a_small_stack` (3-link chain, asserts `Ok` on the same
  small-stack probe). Un-`#[ignore]`d
  `rasterize_svg_use_mutual_reference_cycle_crashes_on_a_small_stack_known_issue` (renamed to
  `..._is_now_rejected_gracefully`), now asserting `Err` instead of just "didn't crash" — CPE-1414's own
  ticket is left open/Deferred (its scope was never separately shipped as code), but the crash it reported no
  longer reproduces.

**Test results (Windows, debug build, this worktree):**
- `crates/server` lib tests: 1707 passed, 0 failed (includes all 14 new `thumb_svg` unit tests).
- `crates/server/tests/thumb_svg_panic_safety.rs`: 8 passed, 0 failed, 0 ignored (was 7 passed + 1 ignored
  before this change) — the mutual-cycle case now runs for real and passes, plus the two new deep-chain cases.
- Full `cargo test --manifest-path crates/server/Cargo.toml`: all suites green (archive/binary-data/parser/
  sample-fixtures/vault/video/etc. panic-safety batteries unaffected).
- `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets -- -D warnings`: clean.
- `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets --features index -- -D warnings`:
  clean (the `index` feature combo CI also exercises; `thumb_svg` itself isn't feature-gated).
- No `Cargo.toml`/`Cargo.lock` changes — no new dependency; `resvg::usvg::roxmltree` (usvg's own public
  re-export) and `resvg::usvg::decompress_svgz` (usvg's own public fn) cover everything needed, so the crate's
  no-new-deps guardrail holds.

## Work Log — attempt 2 (2026-08-07, same worktree/branch `cpe-1437-svg-use-chain-depth`, PR #709)
An independent adversarial security audit of attempt 1 (above) found a real, reproducible THIRD bypass —
confirmed with actual `STATUS_STACK_OVERFLOW` crashes on the 256KB probe — and it changed the whole approach.

**The bypass:** `usvg`'s real native recursion cost is the COMPOSITION of `<use>`-hop count AND each hop's
target's own internal `<g>`-nesting depth, not either dimension alone. Attempt 1's `use_reference_chain_too_deep`
only counted hops; `xml_nesting_too_deep` only bounds a single document's own max literal nesting. Neither
bounds the product. The auditor's concrete reproducers (all confirmed to crash under attempt 1, all passing
BOTH existing caps individually): 10 containers each `<g>`-nested 20 deep chained by 11 `<use>` hops; a single
`<use>` into one 40-deep `<g>` container; and — a pre-existing CPE-1413 hole, unrelated to `<use>` at all —
plain `<g>` nesting alone crashing the probe around depth ~35, during `usvg`'s tree-*conversion* pass (separate
from, and apparently cheaper-per-level than, the raw XML *parse* pass CPE-1413's original nesting-guard sizing
was profiled against), which is UNDER that guard's existing 64-level cap.

**Why depth-prediction was abandoned as the primary defense:** CPE-1414 (three attempts) and this ticket's own
attempt 1 had now EACH been independently defeated by an adversarial reviewer finding a dimension of `usvg`'s
real recursion cost the guard's model didn't cover. Extending either guard with a third dimension (nesting ×
hops) only invites a fourth bypass — trying to model a large, evolving C-like library's exact internal
recursion shape from the outside is inherently brittle. So attempt 2 adopts the ticket's own always-available
alternative fix direction as the PRIMARY, durable guarantee: **run the actual `usvg` work on a thread with a
guaranteed-large stack**, so `usvg`'s own hard recursion cap (1024 levels — which it always enforces regardless
of how the depth is distributed) is reached gracefully instead of ever being reachable by an under-provisioned
caller stack.

**The fix (`crates/server/src/thumb_svg.rs`):**
- `rasterize_svg` now only runs `xml_nesting_too_deep` (a genuinely non-recursive flat byte-scan, provably
  stack-safe at any input depth) on the caller's own thread as a cheap first-pass reject, then hands off to
  `rasterize_svg_on_a_guaranteed_stack(bytes.to_vec(), max_edge)`.
- `rasterize_svg_on_a_guaranteed_stack` spawns a dedicated thread via
  `std::thread::Builder::new().stack_size(RASTERIZE_STACK_SIZE).spawn(...)` and does EVERYTHING else inside
  that closure — including `use_reference_chain_too_deep` (kept as fast defense-in-depth against a
  pathologically long/cyclic chain — cheaper to reject than to fully parse+render — but no longer relied on as
  the stack-overflow bar), the real `usvg::Tree::from_data` parse, and the `resvg::render` call. The result
  (or a graceful `Err` if the closure panics) crosses back via `JoinHandle::join`; a failed thread spawn is
  also just an `Err`, never a panic. **Stack size: 16MiB** (`RASTERIZE_STACK_SIZE`), chosen because it's ~64x
  the 256KB probe that empirically overflows well under usvg's own 1024-level cap, giving several thousand
  levels of headroom under any plausible per-level cost estimate — and now empirically confirmed (see test
  results below) safe against every payload the audit found, plus the pre-existing plain-nesting hole.
- **A subtlety the verification itself caught and is now documented in the module doc comment:**
  `use_reference_chain_too_deep` is NOT "cheap and provably non-recursive" the way `xml_nesting_too_deep` is —
  to mirror `usvg`'s exact entity/DTD decoding it calls the REAL, recursive
  `roxmltree::Document::parse_with_options`, the identical parser class CPE-1413 originally found recurses per
  XML nesting level with no cap of its own. A first pass at this fix left that call on the CALLER's thread
  (reasoning it only ever saw input already passed by the 64-level nesting cap) — a targeted small-stack
  diagnostic during verification proved that wrong: a BARE `roxmltree` parse of a document only ~42 levels
  deep (well under that 64-level cap) already overflows a 256KB stack by itself. Moving
  `use_reference_chain_too_deep`'s call inside the guaranteed-stack closure (as described above) closed this
  immediately — confirmed by rerunning the exact diagnostic and the full battery afterward.

**Test results (Windows, debug build, this worktree, after the fix):**
- `crates/server/tests/thumb_svg_panic_safety.rs`: **13 passed, 0 failed, 0 ignored** (both `--test-threads=1`
  sequential and default-parallel runs) — includes all three of the auditor's exact payloads
  (`rasterize_svg_never_stack_overflows_on_a_composed_chain_of_nested_use_containers_on_a_small_stack`,
  `..._on_a_single_deeply_nested_use_target_on_a_small_stack`,
  `..._on_plain_nesting_under_the_cap_on_a_small_stack` — all now assert `Ok`, i.e. render successfully rather
  than merely fail gracefully, since they're legitimate shapes well within `usvg`'s own bounds), the existing
  cycle/deep-chain/self-reference cases (still `Err` via the fast pre-scan), and two new legit-SVG confirmations
  (`rasterize_svg_renders_a_symbol_sprite_sheet_fine_on_a_small_stack`,
  `..._an_eight_deep_grouped_illustration_fine_on_a_small_stack`).
- `crates/server` lib tests: **1708 passed, 0 failed** (+1 for the new
  `use_chain_guard_boundary_exactly_at_the_cap_is_allowed_one_more_is_rejected` boundary test: a chain of
  exactly `MAX_USE_CHAIN_DEPTH`=128 links is allowed, 129 is rejected).
- Full `cargo test --manifest-path crates/server/Cargo.toml` (default features): all suites green.
- Full `cargo test --manifest-path crates/server/Cargo.toml --features index`: 1751 lib tests + all
  integration suites green (matches CI's `index`-feature test run).
- `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets -- -D warnings`: clean.
- `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets --features index -- -D warnings`: clean.
- No `Cargo.toml`/`Cargo.lock` changes in this attempt either — still zero new dependencies.
- No leftover scratch attack-test file (`tests/zz_sec_audit_attack.rs` was never present in this worktree).

**Cross-references:** CPE-1413 (the original literal-nesting guard, whose sizing this attempt found a small-
stack conversion-pass hole in — now moot since the guaranteed-stack fix covers it regardless) and CPE-1414
(the still-Deferred cycle-guard ticket, whose reproducer remains covered by `use_reference_chain_too_deep`'s
fast-reject AND, as a second independent layer, by this attempt's guaranteed stack).

## Work Log — attempt 3 (2026-08-07, same worktree/branch `cpe-1437-svg-use-chain-depth`, PR #709) — LAST ATTEMPT before parking
A third independent adversarial audit found a real bypass of attempt 2's 16MiB guaranteed-stack fix — this
one overflows even THROUGH the large-stack thread, because it isn't bounded by anything `usvg` itself caps.

**The bypass:** `usvg` has SEVERAL reference-resolution recursions that are bounded only by total element
count (~1,000,000), NOT by the 1024-level cap the `<use>`/nesting composition class (attempt 2) relies on.
Confirmed by reading `usvg-0.45.1`'s own source directly (not just inferring from behavior):
- `clippath::convert` (`parser/clippath.rs` line ~57) self-recursively calls `convert(link, ...)` when a
  `<clipPath clip-path="url(#…)">` itself has a `clip-path`, with only a direct-self-reference guard
  (`link == node`), no depth cap.
- `mask::convert` (`parser/mask.rs`) has the byte-for-byte identical self-recursion shape for `<mask
  mask="url(#…)">`.
- `paint_server::convert_pattern` (`parser/paint_server.rs`) converts a `<pattern>`'s content via the SAME
  general `converter::convert_children` every other element's children go through — so a pattern-filled
  shape nested inside another pattern's content recurses back into `convert_pattern` through ordinary
  converter mutual recursion (not a dedicated self-call, but the same missing-depth-cap effect).
- `marker::convert` (`parser/marker.rs`) converts a `<marker>`'s content the identical way, so a
  `marker-start`-referencing path nested inside another marker's content has the same shape.
- `filter::convert`/`convert_url` (`parser/filter.rs`) don't chain filter-to-filter directly, but a
  `<filter>` can hold a `<feImage href="#element">` referencing an ARBITRARY element via
  `converter::convert_element` (the general entry point) — so an element with its OWN `filter="url(#…)"`
  reached via `feImage` recurses back through the converter into `filter::convert` again, indirectly
  chaining just as deep.
- `usvg`'s only defense against any of these (the various `link == node` self-reference checks) breaks a
  direct 1-hop cycle, not a long ACYCLIC chain — the identical gap CPE-1414 had for `<use>`, just never
  fixed for these five reference types.
- **This DoS already existed on `main` before CPE-1437 ever started** — a 2MB production stack overflows at
  an even shorter chain than the 256KB probe does. CPE-1437 didn't introduce it; closing the small-stack bar
  means closing this too.

Concrete reproducers, confirmed to overflow even the 16MiB guaranteed-stack thread: a clipPath chain (`Ok`
at N=7000, `STATUS_STACK_OVERFLOW` at N=8000, ~686KB SVG) and a mask chain (overflows by N=5000, ~413KB
SVG). Pattern/marker/filter share the identical architectural gap (verified via source reading, not
separately crash-reproduced pre-fix, given the time-boxed nature of this last attempt) and are bounded by
the same generalized guard below.

**Why no stack size can fix this:** unlike the `<use>`/nesting composition class (bounded by `usvg`'s own
1024-level recursion cap, so a big-enough stack always lets `usvg` reach that cap gracefully), these five
reference types are bounded only by `usvg`'s ~1,000,000-element cap — a longer chain always exists that
fits under 1M elements yet overflows any fixed stack. The INPUT must be bounded, not the stack.

**The fix:** generalized the existing non-recursive reference-graph pre-scan (previously `<use>`-only) to
walk ALL SIX reference types in one unified graph, under one unified cap:
- `direct_reference_targets` (new): resolves `<use>`/`<feImage>`'s bare-IRI `href` (reusing
  `resolve_use_href`'s existing `xlink:href`-before-`href` precedence unchanged) PLUS every `url(#id)` found
  in `clip-path`/`mask`/`filter`/`marker-start`/`marker-mid`/`marker-end`/`fill`/`stroke` (via new
  `find_func_iri_ids`, mirroring `svgtypes::FuncIRI::from_str`'s grammar by hand the same way
  `parse_iri_fragment` mirrors `IRI::from_str` — no new dependency). `filter` can legally hold a LIST of
  `url(...)` references (`filter="blur(2) url(#f1) url(#f2)"`), so `find_func_iri_ids` scans for every
  occurrence in the value rather than requiring the whole value to be exactly one reference — deliberately
  MORE permissive than `usvg`'s own single-value grammar, which only makes this guard more conservative,
  never less.
- `hops_from_target` (rewritten from CPE-1437 attempt 1's `use_targets_reached_via`): scans `target`'s
  entire subtree (`target.descendants()`, which includes `target` itself) for any node that is ITSELF
  reference-bearing, keeping that NODE (not its resolved target) as a deferred further hop. **A subtle bug
  caught by this attempt's own boundary test**: an earlier draft used `flat_map(direct_reference_targets)`
  here (resolving each found node's reference immediately) instead of `filter(...).collect()` (keeping the
  node itself) — that silently double-advanced the chain two links per DFS level instead of one, so a
  129-link chain measured as depth ~65 and sailed under the 128 cap.
  `use_chain_guard_boundary_exactly_at_the_cap_is_allowed_one_more_is_rejected` caught this immediately
  (129 stopped being rejected) — fixed by reverting to the `filter`-and-defer-resolution shape, matching
  attempt 1's original `<use>`-only logic exactly, just generalized to all six reference types.
- `hops_from_target` also memoizes per TARGET node (`target_cache: HashMap<Node, Vec<Node>>`) — added
  proactively (not forced by a crash) because many independent triggers can legally share one target (e.g.
  many shapes filled with the same `url(#pattern)`), and without caching that would make this pre-scan's
  own CPU cost quadratic in the number of independent references into one large shared target.
- `reference_chain_too_deep` (renamed from `use_reference_chain_too_deep`): trigger nodes are now every
  element with a non-empty `direct_reference_targets` (not just `<use>` elements) — same DFS/cycle-detection/
  memoization/early-bail machinery as attempt 1, unchanged.
- **Unified cap: `MAX_REFERENCE_CHAIN_DEPTH = 128`** (renamed from `MAX_USE_CHAIN_DEPTH`, value unchanged —
  one cap over the union of all six edge types, not a per-type cap, so a mixed chain crossing reference
  types doesn't get extra budget just by changing type partway through). 128 is ~40-60x below the empirical
  clipPath (N=8000) and mask (N=5000) overflow floors — the tightest constraint now, since those aren't
  bounded by `usvg`'s 1024 cap at all — and still comfortably below that 1024 cap and realistic-SVG-friendly
  (real reference indirection of any of these six kinds is essentially always 1-3 hops).
- The 16MiB guaranteed-stack thread (attempt 2) is KEPT as defense-in-depth for the `<use>`/nesting
  composition class it already closes; `reference_chain_too_deep` now runs inside it (unchanged placement
  from attempt 2, still necessary since it does a real recursive `roxmltree` parse).

**Residual risk, reported per the coordinator's explicit ask rather than silently left closed:** this
pre-scan does NOT attempt to bound a COMBINED attack interleaving several of these six reference types with
independent literal `<g>` nesting inside each hop (the same composition-of-dimensions concern attempt 2
found for `<use>`+nesting, now with six dimensions instead of two). No adversarial reproducer for that
composed shape has been found or confirmed against this codebase in the three attempts so far, but it also
hasn't been proven safe — flagged explicitly in the module doc comment (`crates/server/src/thumb_svg.rs`)
for whoever next touches this guard, rather than assumed closed.

**Tests added (`crates/server/tests/thumb_svg_panic_safety.rs`):** five new payload generators
(`clip_path_chain_svg`, `mask_chain_svg`, `pattern_chain_svg`, `marker_chain_svg`,
`filter_feimage_chain_svg`, each built in the identical alternating-chain shape as the coordinator's
clipPath/mask reproducers) and six new tests: one small-stack rejection test per reference type at N=8000
(`rasterize_svg_never_stack_overflows_on_a_{clippath,mask,pattern,marker,filter_feimage}_chain_on_a_small_stack`,
all assert `Err`) plus one combined positive test
(`rasterize_svg_renders_a_legit_single_level_clip_path_mask_pattern_marker_filter_fine_on_a_small_stack`,
asserts `Ok` for a single legitimate use of each of the five). Also added one unit test in `thumb_svg.rs`
(`use_chain_guard_boundary_exactly_at_the_cap_is_allowed_one_more_is_rejected`) that caught the
double-hop bug described above.

**Test results (Windows, debug build, this worktree, after the fix):**
- `crates/server/tests/thumb_svg_panic_safety.rs`: **19 passed, 0 failed, 0 ignored** (both
  `--test-threads=1` sequential, ~1.4s, and default-parallel, ~1.0s) — includes all six new attempt-3 tests
  plus all 13 tests from attempts 1/2 still green.
- `crates/server` lib tests: **1708 passed, 0 failed** (unit-test count unchanged from attempt 2's count —
  the boundary test already existed from attempt 2 and now correctly exercises the fixed logic).
- Full `cargo test --manifest-path crates/server/Cargo.toml` (default features): all suites green.
- Full `cargo test --manifest-path crates/server/Cargo.toml --features index`: 1751 lib tests + all
  integration suites green.
- `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets -- -D warnings`: clean.
- `cargo clippy --manifest-path crates/server/Cargo.toml --all-targets --features index -- -D warnings`:
  clean.
- No `Cargo.toml`/`Cargo.lock` changes — still zero new dependencies (all six reference types resolved via
  hand-mirrored grammars + `roxmltree`/`resvg::usvg` APIs already in use).
- No leftover scratch attack-test file.

**Cross-references (updated):** CPE-1413 (the literal-nesting guard; this attempt's audit reconfirmed its
sizing has a small-stack conversion-pass gap, now moot under the guaranteed-stack fix) and CPE-1414 (still
Deferred; its cycle reproducer remains covered by `reference_chain_too_deep`'s cycle detection AND the
guaranteed stack, unchanged from attempt 2).
