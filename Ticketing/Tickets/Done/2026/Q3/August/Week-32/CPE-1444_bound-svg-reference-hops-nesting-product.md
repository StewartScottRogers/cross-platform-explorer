---
id: CPE-1444
title: "Complete SVG rasterization DoS hardening: bound the reference hops×nesting PRODUCT for mask/pattern/marker (finish the parked CPE-1437 with the now-known design)"
type: Bug
status: Done
priority: High
component: Backend
tags: [ready, security]
epic: CPE-718
created: 2026-08-07
---
## Why (the re-scope of parked CPE-1437)
CPE-1437 was parked after 3 attempts, but the adversarial auditor (right 3×) fully characterized the last
remaining vector AND the fix — so this is no longer an open design problem. Carry forward PR #709's branch
`cpe-1437-svg-use-chain-depth` (tip `6d544cad` on origin), whose work is all VERIFIED-GOOD:
- usvg rasterization runs on a dedicated 16MiB-stack thread (`rasterize_svg_on_a_guaranteed_stack`);
- a non-recursive pre-scan bounds the reference-CHAIN depth (128 hops) over all six usvg reference
  recursions (use/feImage, clip-path, mask, filter, pattern, marker);
- `<use>` composition is additionally bounded by usvg's ~1M node cap; the cycle + href-encoding bypasses
  (CPE-1414) are closed; 19/19 panic-safety cases green.

## The ONE remaining vector (park reason)
For `mask` / `pattern` / `marker`, usvg descends each hop's per-hop literal `<g>` nesting WHILE the
reference-chain recursion is still on the stack → real recursion depth ≈ **hops × nesting**. The current
guard caps hops (128) and nesting (64) INDEPENDENTLY, but their product 128×64 = 8192 frames overflows the
16MiB thread. Auditor reproducers (verified via `run_on_small_stack` → `rasterize_svg`):
`mask_nested(127, 62)`, `pattern_nested(127, 62)`, `marker_nested(127, 62)` — 127 hops each nested 62 `<g>`
deep (≈7874 frames) → STATUS_STACK_OVERFLOW (0xc00000fd), uncatchable process abort. In DEBUG it overflows
at ~127×35 (≈4500 frames). `clip-path` is ADDITIVE (usvg resolves clip chains separately from group
descent) → safe, keep its higher cap. `<use>` is node-cap-bounded → safe.

## Fix
In the existing non-recursive pre-scan, for the MULTIPLICATIVE reference types (mask/pattern/marker — and
verify filter/feImage's behavior), bound the **combined** cost, not two independent caps: track
**Σ(per-hop literal `<g>` nesting depth)** along the reference chain (or equivalently gate on the
hops×nesting product), and reject (graceful Err) if it exceeds a cap set WELL under the empirical overflow
floor with 3-OS-matrix + debug margin — the DEBUG floor is ~4500 frames, so target a combined-cost cap in
the low thousands (e.g. ≤2048–3000; justify against both the 16MiB release floor ~7874 AND the debug
~4500 floor, since CI runs debug). Keep `clip-path` on the additive (higher) cap and `<use>` as-is. Do NOT
regress the closed vectors or over-reject legit artwork (single-level + shallow references must still
render).

## Tests (the battery is currently FALSELY green on this — no nested-composition case exists)
Add to `thumb_svg_panic_safety.rs`: `mask_nested(127,62)`, `pattern_nested(127,62)`, `marker_nested(127,62)`
(and a couple above/below the new cap) → must now return graceful Err on the small-stack probe, not
overflow. Positive cases: a legit multi-level mask/pattern/marker within the cap still renders Ok. Keep all
19 existing cases green. Also FIX the overclaiming module doc comment (attempt 3's "several thousand levels
of headroom, comfortably past usvg's 1024 cap" is empirically false — reword to describe the product bound).

## If this ALSO fails re-audit → escalate to structural isolation
If a bounded pre-scan still can't close it (yet another multiplicative usvg recursion surfaces), the durable
answer is process isolation: rasterize untrusted SVG thumbnails in a child process with a bounded stack, so
an overflow kills only the child. That is genuinely big-design (cross-platform spawn + bytes→PNG IPC, likely
folds into the sidecar/thumbnail pipeline) — at that point re-file as an epic-scoped ticket rather than
retrying the pre-scan.

## Notes
Build ON `origin/cpe-1437-svg-use-chain-depth` (don't restart from main — reuse its verified-good 16MiB
stack + chain caps). Escalate model to opus (a wrong cap over-rejects real art or misses the vector).
Related: [[CPE-1437]] (parked), CPE-1445 (SVGZ-gzip guard bypass, same file — serialize), epic CPE-718.

---

## Work Log — 2026-08-07 (Done)

Implemented on branch `cpe-1444-svg-product-bound`, based on PR #709's verified-good tip
`6d544cad` (then merged current `origin/main` in, picking up CPE-1446 and resolving the parked
CPE-1437 ticket into Done). Ships as a NEW PR that carries forward / supersedes #709 and closes
the last parked CPE-1437 vector.

### Combined-cost algorithm (the second dimension)
`reference_chain_too_deep`'s explicit-stack post-order DFS already computed, per node, the longest
reference-**hop** chain (memoized `Done { depth }`, capped at `MAX_REFERENCE_CHAIN_DEPTH = 128`). It now
carries a SECOND accumulator in the same walk:

- Each graph edge `A → B` (A reference-bearing, B a reference-bearing node inside the subtree of a target
  A references) is tagged with a **per-hop cost**: for a MULTIPLICATIVE reference it is the referenced
  target subtree's own maximum element-nesting depth (`subtree_nesting_depth`, an iterative explicit-stack
  walk, memoized per target); for an ADDITIVE reference (clip-path, `<use>`) it is `0`.
- Post-order, each node memoizes `cost = max over its edges of (edge_cost + child.cost)` — i.e. the worst
  (deepest) accumulated Σ(per-hop nesting) of any chain starting at that node. A node reachable by many
  chains keeps its worst cost (path-independent, so per-node memoization is correct). Reject the moment any
  node's accumulated cost exceeds `MAX_REFERENCE_COMBINED_COST`, exactly like the existing depth check.
- Cycles are still rejected up front (a revisited `InProgress` node ⇒ unbounded chain), and the open-path
  hop-count early-reject (`stack.len() > max_depth`) is unchanged; per-hop cost is itself bounded by the
  64-level `xml_nesting_too_deep` cap that ran first on the caller thread, so total work stays `O(max_depth)`.

### Cap + justification vs both floors
`MAX_REFERENCE_COMBINED_COST = 2048`. Justified against both empirical STATUS_STACK_OVERFLOW floors:
~2.2× below the **~4500-frame debug** floor (CI builds debug — the binding constraint) and ~3.8× below the
**~7874-frame release** (16MiB thread) floor; also far below the independent-cap envelope 128×64 = 8192.
Above legitimate artwork by ~34×: real SVGs use a handful of reference hops and a handful of group levels,
so even a generous "5 hops × 12-deep" illustration costs ~60. Because `xml_nesting_too_deep` (≤64) always
runs first, per-hop cost ≤ 64, so the product cap only bites in the ~32–128-hop-with-deep-nesting window.

### Multiplicative vs additive determination
Multiplicative (contribute to the product cost): **mask**, **fill/stroke → pattern**, **marker-start/mid/end**,
and **filter + feImage**. Additive (hop-capped only, cost 0): **clip-path** and **`<use>`**.
`filter`/`feImage` determination: a `<feImage href="#el">` resolves an ARBITRARY element through the same
general `converter::convert_element`/`convert_children` that descends the target's subtree on-stack while
the filter-reference frame is live — the identical multiplicative shape as `mask`, so treated as
multiplicative (also the ticket's "treat as multiplicative if in doubt"). `clip-path` is left additive
because usvg resolves a clip chain separately from group descent (`clippath::convert` chains clip-to-clip
but does not descend each clip's inner group nesting on the clip-chain frame), so its real cost is the hop
count alone — kept on `MAX_REFERENCE_CHAIN_DEPTH`. `<use>` cloning is bounded by usvg's ~1,000,000-node /
1024-`<use>`-depth caps and survives the 16MiB stack (the composition class CPE-1437 attempt 2 closed).

### Tests / verification (all SYNCHRONOUS, DEBUG — where the floor is lowest)
- `cargo build` — clean.
- `cargo clippy --all-targets -D warnings`, default AND `--features index` — clean (factored the DFS frame
  into `ChainEdge`/`DfsFrame` type aliases to satisfy `type_complexity`).
- `cargo test` (whole crate) — **1712 lib + all integration binaries pass, 0 failed**.
- Panic-safety battery (`thumb_svg_panic_safety.rs`, debug) — **24/24 pass**, now including new
  `mask_nested(127,62)` / `pattern_nested(127,62)` / `marker_nested(127,62)` → **graceful Err** on the
  256KB `run_on_small_stack → rasterize_svg` path (no overflow); the combined-cost boundary straddle
  (`mask_nested(32,62)` renders **Ok**, `mask_nested(33,62)` → **Err**); and a legit multi-level
  mask+pattern+marker(4,8) positive that still **renders Ok**. All 19 prior cases stay green, plus
  clip/mask/pattern/marker/filter chain(8000) → Err and the composed-`<use>` positives.
- `thumb_svg` module unit tests — **26/26 pass**, including 4 new combined-cost guard tests: the real
  reproducer rejected by the COST cap (not the hop cap — proven by re-running with `usize::MAX` cost cap so
  127 hops is allowed), the 32/33-hop boundary, and clip-path staying additive (127-hop nested clip NOT
  cost-rejected, while a 200-hop clip is still hop-cap rejected).

Also reworded the overclaiming `RASTERIZE_STACK_SIZE` module doc ("several thousand levels of headroom,
comfortably past usvg's 1024 cap") — empirically false per attempt 3 — to describe the product/combined-cost
bound and note clip = additive / `<use>` = node-capped; and updated the module-level "residual risk"
paragraph attempt 3 left open to record it as CLOSED by this ticket. No new dependencies; graceful Err
throughout, never a panic.
