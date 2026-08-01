---
id: CPE-1211
title: "Fix red main: src-tauri image-similarity test used a featureless fixture (post-CPE-1205)"
type: bug
component: Testing
priority: high
status: Done
tags: ready
created: 2026-08-01
epic: CPE-997
---

## Summary
**Base-red hotfix.** CPE-1205's featureless-image guard (excludes near-uniform images from near-dup grouping)
correctly excluded the `find_similar_images_collect_groups_a_fixture` test's embedded `TINY_PNG` — a 4x2
**gradient** (featureless, dHash popcount ~0). So the src-tauri test asserted "the identical pair forms one
group" but got 0 groups → `cargo test` (src-tauri) red on all CI legs from CPE-1205 onward. CPE-1205's gauntlet
ran `cargo test -p cpe-server` (which it updated to structured fixtures) but NOT the src-tauri suite where this
integration test lives, so it slipped past review; the 3-OS CI matrix caught it post-merge.

## Fix
- `src-tauri`: add a dev-only `image` dependency (png feature; not a runtime dep) and replace the hand-embedded
  featureless `TINY_PNG` with a generated **structured checkerboard** PNG (balanced dHash popcount → not
  excluded by the guard). Two byte-identical structured copies → one near-dup group, as the test intends.

## Acceptance Criteria
- [x] `cargo test` (src-tauri) green — 81/81 incl. `find_similar_images_collect_groups_a_fixture`; clippy clean.
- [ ] main CI green on the fix commit (confirm post-merge).

## Work Log
- 2026-08-01 — Foreman base-red hotfix. **Process lesson:** a change to `cpe-server` perceptual/logic must run
  BOTH `cargo test -p cpe-server` AND the src-tauri `cargo test` (its integration tests depend on cpe-server
  behavior) — add to the backend-change gauntlet checklist.
