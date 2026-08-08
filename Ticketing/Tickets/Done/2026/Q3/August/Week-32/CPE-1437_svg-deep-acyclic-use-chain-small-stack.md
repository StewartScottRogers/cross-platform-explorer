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
2026-08-07 (workshift worker, branch `cpe-1437-svg-use-chain-depth`) — Implemented in
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
