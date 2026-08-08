---
id: CPE-1444
title: "Complete SVG rasterization DoS hardening: bound the reference hops×nesting PRODUCT for mask/pattern/marker (finish the parked CPE-1437 with the now-known design)"
type: Bug
status: Backlog
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
