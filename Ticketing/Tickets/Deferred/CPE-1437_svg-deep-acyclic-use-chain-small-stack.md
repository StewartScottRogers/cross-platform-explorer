---
id: CPE-1437
title: "SVG deep ACYCLIC <use> chain overflows the 256KB probe stack (small-stack bar)"
type: Bug
status: Deferred
priority: Low
component: Backend
tags: [deferred-internal]
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

## Work Log — PARKED after 3 attempts (2026-08-07 workshift)
Attempted 3× on branch `cpe-1437-svg-use-chain-depth` (PR #709, left open/draft, tip `6d544cad`). An
independent adversarial security auditor found a NEW adjacent vector on each attempt — this is progressive
narrowing, not a repeated single failure:
1. **Attempt 1** — non-recursive `<use>` reference-chain depth cap (128). Auditor bypass: use-hop count and
   XML-nesting are counted independently, but usvg recursion ≈ their COMPOSITION → a 10-container × 20-nest
   chain overflowed.
2. **Attempt 2** — ran usvg on a dedicated 16MiB-stack thread (+ moved the guard's own roxmltree parse onto
   it). Closed the `<use>`/composition class. Auditor bypass: `clipPath`/`mask` reference CHAINS recurse in
   usvg's converter phase, NOT bounded by usvg's 1024 cap — a ~400–700KB SVG (5000–8000-element chain)
   overflowed even 16MiB.
3. **Attempt 3** — extended the pre-scan to bound all SIX usvg reference recursions (use/clip/mask/filter/
   pattern/marker) under one cap of 128 hops. Closed the chain vectors for all 6. **Remaining vector (the
   park reason):** for `mask`/`pattern`/`marker`, usvg descends per-hop `<g>` nesting WHILE the chain
   recursion is on the stack, so real depth ≈ hops × nesting. Two independent caps (128 hops × 64 nesting)
   multiply to 8192 frames; a `mask_nested(127, 62)` (~7874 frames) overflows the 16MiB thread (in DEBUG it
   overflows outright at ~127×35). `clip-path` is additive (safe); `<use>` is bounded by usvg's ~1M node cap
   (safe) — only the 3 multiplicative types remain.

**Auditor's conclusion (proven right 3×):** a fixed stack size cannot close an input-scaled hops×nesting
recursion — the INPUT PRODUCT must be bounded. That is a design change to the pre-scan model, so per the
circuit breaker this is parked and **re-scoped into [[CPE-1444]]** (bound Σ per-hop-nesting / the hops×nesting
product for the multiplicative types, building on this branch's verified-good 16MiB stack + chain caps). If
that also fails re-audit, the durable answer is process isolation (also in CPE-1444). Full reproducers + usvg
source locations are in the PR #709 audit records.

