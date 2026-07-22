---
id: CPE-883
title: Fix Agent Board duplicating an archived ticket when moved to the Done column
type: bug
component: Sidecar
priority: medium
tags: ready
epic: CPE-850
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
`agent-board::board::move_card` removed the source ticket file only when the **column** changed
(`from != to`). But an archived Done ticket lives in a nested `Done/YYYY/QN/…` folder while its move
destination is top-level `Done/`. Moving an archived ticket to the **Done** column therefore has
`from == to == "Done"` yet `src != dest`, so the nested original was left in place while a new top-level
file was written — the ticket then existed **twice** (once in the active board, once still in the archive).
Introduced alongside the CPE-864 recursive archived-ticket support.

Fix: remove the source whenever the written path differs from it (`src != dest`), not just on a column
change. This is a no-op when a top-level ticket is rewritten in place, still removes on a cross-column move,
and now also removes the nested original when un-archiving into Done.

## Acceptance Criteria
- [x] Moving an archived (nested Done) ticket to the Done column removes the nested original — ticket exists
      exactly once across the active board + archive.
- [x] Cross-column moves and in-place same-file rewrites still behave (never lose the ticket; write before
      remove).
- [x] Full `agent-board` suite (16) + `cargo clippy --all-targets -D warnings` green.

## Work Log
- 2026-07-22 (autonomous) — Found the duplicate while auditing the sidecar's hand-rolled board logic.
  Switched the removal guard from column identity to path identity; added a regression test that duplicates
  against the old code. 16/16 tests pass; clippy clean.
