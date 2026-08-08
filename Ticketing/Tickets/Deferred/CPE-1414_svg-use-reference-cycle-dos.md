---
id: CPE-1414
title: "Security: SVG mutual <use>/<symbol> reference cycle can stack-overflow (low real risk; small-stack DoS)"
type: Bug
status: Deferred
priority: Low
component: Backend
tags: [deferred-internal]
epic: CPE-718
created: 2026-08-07
---

## PARKED 2026-08-07 (workshift circuit-breaker: 3 attempts, each failed adversarial re-review)
An attempt at this fix (PR #700, branch `worktree-agent-cpe1414`, left as a DRAFT) reached the 3-attempt cap. An
adversarial security reviewer found a real 256KB-stack-overflow bypass on **every** attempt — each fix was sound
as far as it went, but the hand-rolled `<use>` edge-extraction kept not-quite-matching what usvg actually
resolves. **Low real-world risk (256KB test-probe only; prod is 2MB + usvg caps), so parked rather than burning
more shift budget.** The architecture is RIGHT and the remaining fix is now known and small — a fresh focused
session should land it fast.

**The three bypasses found (all reproduce STATUS_STACK_OVERFLOW on the 256KB probe, all SAFE on prod 2MB stack):**
1. **Entity-encoded href** (`xlink:href="&#35;b"` / hex `&#x23;`) — a byte-scan guard reads raw `&#35;b` (no `#`
   prefix → no edge) while roxmltree/usvg decode it to `#b`. Also entity-encoded `id` (`<symbol id="&#97;">`).
2. **Internal-subset DTD entity** (`<!DOCTYPE svg [<!ENTITY r "#b">]>` + `href="&r;"`) — the hand-rolled entity
   decoder only knew numeric + the 5 predefined entities; usvg parses roxmltree with `allow_dtd: true`.
3. **`href` vs `xlink:href` precedence** — usvg's `resolve_href` (usvg-0.45.1 `parser/svgtree/parse.rs:533`) is
   `node.attribute((XLINK_NS,"href")).or_else(|| node.attribute("href"))` (namespace priority: xlink FIRST). The
   attempt-3 guard used `attributes().find(|a| a.name()=="href")` (local-name, source-FIRST), so a
   `<use href="#leaf" xlink:href="#b">` (both present, plain first) made the guard read `#leaf` while usvg read
   `#b`. Payload: two symbols each `<use href="#leaf" xlink:href="#other">` forming an a↔b cycle → guard sees no
   cycle, usvg expands it → overflow.

**What attempt 3 got RIGHT (keep it — it's the sound base):** stop hand-parsing; mirror usvg's preprocessing
(SVGZ `1f 8b` gzip-decompress → UTF-8) and parse with the SAME `roxmltree::Document::parse_with_options(text,
ParsingOptions{ allow_dtd: true, ..default })` usvg uses (roxmltree pinned `=0.20.0`, usvg's exact version — no
dup), then walk that tree with the existing non-recursive explicit-stack DFS. This killed bypasses 1 & 2 (the
whole entity/DTD divergence class). Reviewer independently verified usvg's SVGZ + ParsingOptions match byte-for-byte.

**Exact remaining fix (~2 lines) to land it:** in the tree walk, extract each `<use>`'s target with usvg's
precedence — `node.attribute(("http://www.w3.org/1999/xlink","href")).or_else(|| node.attribute("href"))` (xlink
FIRST, then plain href) — NOT local-name/source-order. Ideally also parse the value via `svgtypes::IRI::from_str`
(as usvg does) instead of `.trim().strip_prefix('#')` to stay bit-exact on fragment extraction. Add the
precedence payload above as a regression test. Then re-run the adversarial evasion review one more time; if it's
clean (edge-extraction now mirrors `resolve_href` exactly), un-draft/merge PR #700.

**Reproducers to un-`#[ignore]` / add when fixed:** the entity, DTD-entity, and href-precedence cycle payloads,
each proven flagged + non-overflowing on the 256KB `run_on_small_stack` probe. Also see [[CPE-1437]] (a separate,
non-cycle deep-acyclic `<use>`-chain small-stack overflow found during this review).

**2026-08-07 update — the mutual-cycle crash no longer reproduces, but this ticket's own scope was never
separately shipped as code.** CPE-1437 (a different, narrower ticket — bounding `<use>` reference-CHAIN
*depth*, not cycles per se) landed a fresh, non-recursive reference-graph walk in
`crates/server/src/thumb_svg.rs` (`use_reference_chain_too_deep`) built from scratch using this ticket's
hard-won href-resolution findings above (SVGZ+`allow_dtd`+xlink-precedence). Because that walk's DFS treats
a node revisited while still `InProgress` as a cycle (an unbounded chain by construction) and rejects it
exactly like a too-deep chain, it *incidentally* also catches this ticket's own mutual-`<symbol>` cycle
reproducer — `rasterize_svg_use_mutual_reference_cycle_crashes_on_a_small_stack_known_issue` in
`thumb_svg_panic_safety.rs` is now un-`#[ignore]`d (renamed `..._is_now_rejected_gracefully`) and passes,
asserting a graceful `Err` instead of "didn't crash". Left this ticket Deferred rather than moving it to Done:
its own explicit scope — a dedicated *cycle* guard, as opposed to CPE-1437's chain-depth guard — was never
separately authored or reviewed as its own change, so closing it here would overstate what was actually
verified under this ticket's own name. A future pass can either formally fold this into CPE-1437's PR history
and close it, or leave it Deferred as "superseded in practice."

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
