---
id: CPE-1194
title: "Macro undo fidelity: trash-based convert restore + snapshot-based tag inverse"
type: chore
component: Backend
priority: low
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-739
---

## Summary
Two undo-fidelity gaps from the CPE-1187/1188 review (PR #498), now documented in code, to be truly fixed here:

1. **Convert undo is a lossy re-encode.** `macro_convert_in_place` deletes the original, so undo re-encodes back
   (quality loss), not a byte-exact restore. Route the original to the OS trash on convert so undo can restore
   the real bytes.
2. **Tag inverse can drop a pre-existing tag.** `untag` on undo removes the label regardless of whether the
   user had it before the run. Snapshot the pre-run tag state and restore exactly on undo.

## Acceptance Criteria
- [ ] Convert-undo restores the original bytes (via trash), not a re-encode; `cargo test` proves byte-equality.
- [ ] Tag-undo restores the exact pre-run tag set (a pre-existing label survives undo).
- [ ] `cargo test` + clippy green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift) from the PR #498 review findings 3a/3b. The rollback-honesty blocker
  (finding 2) was fixed inline in #498; these two fidelity improvements are the follow-up.
