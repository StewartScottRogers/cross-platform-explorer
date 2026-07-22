---
id: CPE-864
title: Make the application aware of archived (subfoldered) Done tickets
type: bug
component: Sidecar
priority: high
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
`Tickets/Done/` is now **archived into date subfolders** (`Done/YYYY/QN/MonthName/Week-NN/`, and this will
keep happening as tickets close — see the auto-archive work). But the application's ticket readers list a
column **non-recursively**, so archived Done tickets are **invisible to the app**: the Agent Board's Done
column shows only the handful of un-archived (root-level) tickets, and finding/moving an archived card fails.

This is a regression introduced by reorganising `Done/` — the app must be taught that Done tickets can live
in subfolders.

## Where it breaks (confirmed)
- **Agent Board sidecar** — `sidecar/agent-board/src/board.rs`:
  - `read_board()` (~L149) does `fs::read_dir(Tickets/<column>)` and keeps only direct `*.md`, so subfolders
    (`2026/…`) are skipped → archived Done cards never load.
  - `find_card_file()` (~L178) and `move_card()` scan the column dir non-recursively → an archived card
    can't be found or moved.
- **In-process board reader** — audit `src-tauri/src/lib.rs` (the `board_cards` / `ticket_board` path used by
  the in-process `BoardView`) for the same non-recursive assumption.

## Acceptance Criteria
- [x] Column reads descend **recursively**: the sidecar gains `read_archived` (nested Done) and a recursive
      `find_card_file`/`move_card`; the in-process `board_move` finds recursively too. `board_cards`/
      `read_board` stay top-level (active), matching the CPE-531 split.
- [x] Reorg subfolders (`2026`, `Q3`, `July`, `Week-NN`) are never mistaken for columns/tickets (columns are
      the fixed `COLUMNS`; subfolders are only walked *within* a column).
- [x] Display treatment: recent (top-level) Done stays inline; archived tickets collapse behind a
      **"Archived (N)"** toggle in the sidecar board (`/api/archived` + a lazy toggle), mirroring the
      in-process BoardView. Aware ≠ dumping hundreds of cards.
- [x] Unit tests cover a nested archived ticket: not in the active board, surfaced by `read_archived`, and
      still findable + movable (reopened out of the archive).
- [x] `cargo test` (15) + clippy `--all-targets -D warnings` green in **both** feature modes. (GUI check of
      the "Archived (N)" toggle available in a sidecar build.)

## Notes
- Sibling work: the **auto-archive skill** (moves closed tickets into subfolders automatically) creates the
  ongoing need for this; and the CLI ticketing skills that read `Done/` in prose should likewise glob
  recursively when they count/scan Done.
- Consider whether the board should treat any deeply-nested `.md` under a column as that column's ticket
  (simplest), vs. a dedicated `Done/Archive/` convention.
