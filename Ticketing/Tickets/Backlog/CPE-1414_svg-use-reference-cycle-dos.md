---
id: CPE-1414
title: "Security: SVG mutual <use>/<symbol> reference cycle can stack-overflow (low real risk; small-stack DoS)"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready]
epic: CPE-718
created: 2026-08-07
---

## Problem (CPE-1413 / PR #688 — CONFIRMED, reported not fixed)
`crates/server/src/thumb_svg.rs`: a 2-hop mutual `<use>`/`<symbol>` reference cycle (two symbols each
`xlink:href`-ing the other) crashes a 256KiB thread stack via recursion. usvg only guards DIRECT self-reference
(`<use href="#self">`, confirmed safe) and one-hop back-references; a 2-hop cycle falls through to usvg's own
`depth > 1024` cap whose per-level stack cost is too high for a small stack. CPE-1413's depth-guard pre-scan does
NOT catch this (it's a reference cycle, not literal nesting depth). **Confirmed SAFE on a 2MB stack (this app's
Tokio spawn_blocking default) even in debug → low real-world risk.** Reproducer exists as an `#[ignore]`d test in
`thumb_svg_panic_safety.rs` (`..._use_mutual_reference_cycle_crashes_on_a_small_stack_known_issue`); normal
`cargo test` doesn't run it.

## Fix direction
Needs a non-recursive `<use>`/`xlink:href` reference-cycle detector (build the reference graph, reject a cycle)
BEFORE usvg resolves them — deliberately deferred in CPE-1413 because a hand-rolled cycle detector is hard to get
"clearly correct" (same fragility that caused CPE-1398's own follow-up bypass). Options: pre-scan the id→href
graph for cycles and reject; or run rasterize_svg on a guaranteed-large-stack thread as defense-in-depth. Low
priority given the 2MB-stack safety, but it IS a confirmed crash on small stacks. Un-`#[ignore]` the reproducer
once fixed.
