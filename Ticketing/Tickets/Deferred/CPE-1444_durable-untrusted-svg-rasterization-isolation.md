---
id: CPE-1444
title: "Durable structural fix: bound/isolate untrusted SVG rasterization against ALL usvg resource-exhaustion recursions (not just per-vector pre-scan)"
type: Bug
status: Deferred
priority: Medium
component: Backend
tags: [big-design, security]
epic: CPE-718
created: 2026-08-07
---
## Why (found during CPE-1437 adversarial re-audit, PR #709)
CPE-1437 closes SVG stack-overflow DoS **vector by vector** with a non-recursive reference-chain
pre-scan (`<use>`, then clip-path/mask/filter/pattern/marker chains) + a 16MiB rasterization stack.
That works, but it is **inherently whack-a-mole**: usvg has multiple element-count-scaled resolution
recursions NOT bounded by its 1024 svgtree cap (the re-audit found `clippath::convert` and
`mask::convert` recurse on `clip-path`/`mask` reference *chains*, bounded only by ~1M element count; a
~400–700KB SVG of a 5000–8000-element chain overflows even a 16MiB thread → uncatchable
STATUS_STACK_OVERFLOW → whole-process abort). Every future usvg version can add another such recursion,
and a stack overflow is uncatchable, so a pre-scan that must enumerate every vector is fragile.

## The durable options (design spike needed — hence Deferred/big-design)
Pick the robust structural guard so we stop chasing individual usvg recursions:
1. **Post-parse tree depth/complexity validator** — after `usvg::Tree::from_data` (which is itself the
   risky recursive step... so this may need usvg's own bounded parse), walk the resolved tree
   iteratively and reject if any reference-resolution depth / total node count exceeds a cap. Problem:
   the overflow can happen *inside* `from_data`, before we can validate — so this alone may be
   insufficient without also bounding the input first.
2. **Process isolation** — rasterize untrusted SVG thumbnails in a **separate child process** with a
   bounded stack/memory/time; a stack overflow there kills only the child, the app survives and shows a
   "couldn't render" placeholder. Most robust (covers ALL recursions + memory + CPU DoS at once, and any
   future usvg regression), but the biggest lift (IPC of bytes→PNG, child lifecycle, Windows/macOS/Linux
   spawn, sidecar-style contract). Likely the right long-term answer; possibly folds into the existing
   sidecar/thumbnail-pipeline architecture.
3. **Input size/complexity budget** — cap untrusted-SVG byte size + element count + reference-edge count
   up front (cheap, coarse) as a blunt backstop under whatever per-vector pre-scan exists.

## Acceptance
A design decision (spike → chosen approach) that makes untrusted-SVG rasterization robust against
resource-exhaustion DoS **as a class**, not vector-by-vector, with a test that a pathological SVG of ANY
of the known recursion types (and an oversized/element-bomb one) is handled gracefully without a
process crash. Coordinate with the CPE-1437 pre-scan (this supersedes/backstops it) and epic CPE-718
(thumbnail pipeline).

## Notes
Deferred (needs a design spike + likely cross-platform process work), not Backlog. The immediate
per-vector bleeding is stopped by CPE-1437; this is the durable structural close-out. See PR #709
re-audit for the exact usvg source locations and reproducers.
