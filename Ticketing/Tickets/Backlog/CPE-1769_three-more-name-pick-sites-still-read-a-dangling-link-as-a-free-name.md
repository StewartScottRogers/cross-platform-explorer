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

- [x] All three sites treat a dangling link (and an NTFS junction) as **occupied**.
- [x] They call one shared helper in `crates/server/src/fsutil.rs`. No new local copy of the stat expression.
- [x] Each site has a test that asserts the **harm** — what the batch/snapshot actually wrote, and whether
      the link survived — **before** unwrapping the `Result`. Every defect in this family fails by
      succeeding, so an assertion after an `unwrap` is unreachable exactly when it matters.
- [x] Reverting each fix reds a **distinct** test whose message names the write-through, not a generic
      mismatch.
- [x] Tests clean up via a `Drop` guard armed **before** the assertions (CPE-1693), not a trailing
      `remove_dir_all`.
- [x] The junction leg is covered, not just the symlink leg — an unprivileged Windows runner stages a
      junction, and `remove_file` refuses one with PermissionDenied while `remove_dir` succeeds. That
      asymmetry blocked PR #924 and will block these too.

## Notes

Found by the Reviewer on **PR #924 / CPE-1715**, 2026-08-17, during the batched sprint. Related: CPE-1715
(the two sites already fixed), CPE-1770 (the refusal-shaped siblings), CPE-1705 (the consolidation this
should not undo), CPE-1734, CPE-1765.

## Work Log

**2026-08-20** — All three sites fixed, branch `cpe-1769-dangling-link-name-picks` off latest `origin/main`
(head `c1b3bfff`, after CPE-1765/#968 merged). The shared helper the ticket asked for
(`name_pick_slot_probe`/`classify_link_presence`) had already landed in CPE-1715/PR #924 — confirmed live
on current main (`crates/server/src/fsutil.rs`), so no prerequisite work was needed.

- `batch_media.rs::plan` — one-line swap: `Path::new(&output).try_exists()` →
  `name_pick_slot_probe(Path::new(&output))`, feeding the same `classify_target_slot` the loop already used.
  A dangling link now folds into `Slot::Taken`, so the loop advances to the next candidate exactly as it
  does for a real occupied file.
- `batch_execute.rs::classify_output_occupancy`/`output_occupancy` — added a fourth `OutputOccupancy::Link`
  state (an occupancy classifier, not a picker, needs its own verdict rather than folding into `Free` or
  `File`) and a third `link_present` closure, consulted only when `try_exists` could not itself prove
  occupancy. Reuses `classify_link_presence` for the taxonomy — no new stat-taxonomy code, only the
  unavoidable IO call itself (same split every sibling in this family uses). `is_foreign_overwrite` now
  treats `Link` exactly like `Unknown`: refuse without consent.
- `snapshot_capture.rs` blob-write loop — swapped `dest.try_exists()` for
  `name_pick_slot_probe(&dest)` feeding the same `classify_target_slot`/`Occupied → continue` policy the
  site already had for a real pre-existing blob. A link at a blob's hashed name is now skipped (not written
  through), consistent with the existing "already on disk, benign to skip" rationale for a genuine occupant.

5 new tests (`cpe_1769_*`), one dangling-link fixture each via the existing `crate::fsutil::make_dangling_link`
(falls back to an NTFS junction on this machine — confirmed unprivileged: `New-Item -ItemType SymbolicLink`
fails with "Administrator privilege required", so every new test actually exercised the **junction** leg,
not the symlink leg). Each red-proofed individually by reverting only its site's one-line fix, rerunning,
observing the named failure, then restoring the fix — see the PR description for the exact reverted line and
failure message per site. `cargo clippy --all-targets -- -D warnings` and `cargo test` both clean for
`cpe-server` (2277 passed, 0 failed, plus all five other test binaries green). `src-tauri` untouched, so its
two feature-mode gates and the specta bindings regen do not apply.

One nuance worth recording plainly rather than overclaiming: at the `batch_execute.rs` site, actual byte
escape was already prevented by a separate, pre-existing write-time link guard inside the shared write path
(`execute_one`'s "a batch never writes through a link" refusal) — reverting only the occupancy-classifier fix
does not let bytes land through the link. What the fix restores is the **upfront confirmation gate**: without
it, a dangling link at the output let the whole batch proceed without asking for overwrite confirmation, and
that one file was silently skip-listed with a per-item reason inside an otherwise-"successful" report instead
of the batch being refused outright the way a real occupied file already is. That distinction is exactly what
the ticket's own wording flags for this site ("the write-through goes unconfirmed") — not "bytes escape."
