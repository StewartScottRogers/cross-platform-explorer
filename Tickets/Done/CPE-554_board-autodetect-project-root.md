---
id: CPE-554
title: "Agent Board auto-detects the project root by walking up to a Tickets/ folder"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-503
estimate: 1-2h
created: 2026-07-16
closed: 2026-07-16
---

## Summary
Follow-on to [[CPE-551]]. That ticket made the board's root choosable + sticky and added an empty-state.
This makes it *just work* more often: when you open the board while browsing anywhere **inside** a project
(e.g. `…/repo/src/lib`), it should find the enclosing project (the nearest ancestor with a `Tickets/`
folder) automatically, instead of only working at the exact repo root or requiring a manual pick.

## Acceptance Criteria
- [x] A backend `find_project_root(start)` command walks up from `start` (inclusive) and returns the
      nearest ancestor directory containing a readable `Tickets/` folder, else `None`.
- [x] The pure walk-up is unit-tested (Rust): found from deep inside, found at the root, `None` when no
      ancestor has `Tickets/`.
- [x] `BoardView` uses it on open: when there is no saved board root, auto-detect from the current folder;
      fall back to the current folder (→ empty-state) when nothing is found. A saved/chosen root still wins.
- [x] `npm run check` clean; frontend test covers the auto-detect-on-open path; `cargo` build/clippy/test
      green (CI).

## Resolution
Pure `ticket_board::nearest_project_root(start) -> Option<PathBuf>` walks up from `start` (inclusive) to
the nearest ancestor with a readable `Tickets/` dir (read-only; a missing/denied dir just isn't a match).
Exposed as the `find_project_root` Tauri command (registered in `generate_handler!`). `BoardView` calls it
in `onMount` when there's no saved `cpe.boardRoot`, so opening the board while browsing anywhere inside a
project auto-loads that project; a saved/chosen root still wins, and no match falls back to the current
folder (→ CPE-551 empty-state). Priority: **saved root > auto-detected project > current folder.** Tests:
1 Rust (`nearest_project_root_walks_up_to_the_tickets_folder`, temp-dir) + 1 frontend (auto-detect-on-open)
added to the CPE-551 suite. Verified: Rust test + `cargo clippy -D warnings` clean; `npm run check` 0/0;
full frontend suite 581 passed. Added `tempfile` as a src-tauri dev-dependency. (Live GUI re-verify batched
with the other nightshift frontend fixes.)

## Notes
Complements CPE-551's sticky/choosable root: saved root > auto-detected project > current folder.
Read-only + never fails (a missing/denied dir just isn't a match), consistent with the board's other
skip-on-error commands.
