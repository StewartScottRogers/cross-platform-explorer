---
id: CPE-1769
title: Three more name-picking sites still read a dangling link as a free name
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

Found by the **PR #924 (CPE-1715) review**, which audited every "is this name free?" probe in the codebase
rather than only the two the ticket named. CPE-1715 fixed `unique_target` and `resolve_conflict`. These
three have the identical defect and no ticket.

| Site | Shape |
|---|---|
| `crates/server/src/batch_media.rs:2109` (`plan`) | Name-picking loop whose own comment says *"Mirrors `unique_target`"*. Uses `classify_target_slot(&Path::new(&output).try_exists())`. A dangling link at the output reads **Free** and the batch writes there. |
| `crates/server/src/batch_execute.rs:225` (`classify_output_occupancy`) | `Ok(false) → Free` with no link check, so `is_foreign_overwrite` reports "nothing there" and the write-through goes unconfirmed. |
| `crates/server/src/snapshot_capture.rs:165` | Blob-store destination; a dangling link there makes `fs::copy` write through it. Low harm because the store is content-addressed, but the same shape. |

`batch_media.rs:2109` is the closest sibling of the two CPE-1715 just fixed — a name-picking loop that
copied `unique_target`'s logic by hand and therefore copied its bug.

## Why this is filed separately rather than folded into CPE-1715

CPE-1715's acceptance criteria named two call sites and it satisfied them. Shipping it while four unticketed
siblings stand is the pattern this family keeps re-filing — CPE-1705's own doc says it plainly: *"twelve
copies of the same check is how the thirteenth gets missed."* Writing them down is the point.

## What to do

- **Do not add a fourteenth copy.** CPE-1715's review also required its new probe to move into
  `crates/server/src/fsutil.rs` next to `classify_symlink_slot`, as
  `name_pick_slot_probe` / `classify_link_presence`. Once that lands, these three sites call it. If it has
  not landed yet, that is the prerequisite — do it first rather than working around it.
- Note that `batch_media` lives in `crates/server`, so it can only reach a shared helper if the helper is in
  `cpe-server` and not in the app adapter. That is the whole argument for the move.
- Decide per site what "occupied" should mean for its caller — a name-picking loop advances to the next
  candidate; an occupancy classifier feeds an overwrite decision. Same probe, different consequence. State
  each.

## Acceptance criteria

- [ ] All three sites treat a dangling link (and an NTFS junction) as **occupied**.
- [ ] They call one shared helper in `crates/server/src/fsutil.rs`. No new local copy of the stat expression.
- [ ] Each site has a test that asserts the **harm** — what the batch/snapshot actually wrote, and whether
      the link survived — **before** unwrapping the `Result`. Every defect in this family fails by
      succeeding, so an assertion after an `unwrap` is unreachable exactly when it matters.
- [ ] Reverting each fix reds a **distinct** test whose message names the write-through, not a generic
      mismatch.
- [ ] Tests clean up via a `Drop` guard armed **before** the assertions (CPE-1693), not a trailing
      `remove_dir_all`.
- [ ] The junction leg is covered, not just the symlink leg — an unprivileged Windows runner stages a
      junction, and `remove_file` refuses one with PermissionDenied while `remove_dir` succeeds. That
      asymmetry blocked PR #924 and will block these too.

## Notes

Found by the Reviewer on **PR #924 / CPE-1715**, 2026-08-17, during the batched sprint. Related: CPE-1715
(the two sites already fixed), CPE-1770 (the refusal-shaped siblings), CPE-1705 (the consolidation this
should not undo), CPE-1734, CPE-1765.
