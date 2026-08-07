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
