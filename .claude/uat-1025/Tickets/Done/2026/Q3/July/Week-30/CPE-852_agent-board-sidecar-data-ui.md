---
id: CPE-852
title: Agent Board sidecar — Kanban over Tickets/ (read + move cards)
type: feature
component: Backend
priority: medium
status: Done
tags: needs-prereq
created: 2026-07-21
closed: 2026-07-21
epic: CPE-850
estimate: 4h+
---

## Summary
Second child of CPE-850. Make the sidecar's served UI a real, interactive Agent Board: read the project's
`Tickets/` folders (via the granted `context` root from the host) into columns/cards, render the Kanban,
and move a card between columns (moves the ticket file + updates its `status:` frontmatter). The
card/column/frontmatter logic is small — reimplement in the sidecar or lift into a tiny contract-free
shared crate (the sidecar must not depend on `cpe-server`/the app).

## Acceptance Criteria
- [x] The sidecar reads `Tickets/{Backlog,Doing,Blocked,Deferred,Done,…}` under the context root into
      typed cards; the served UI renders the columns.
- [x] Dragging/moving a card to another column moves the file and rewrites `status:` (same effect as the
      in-app board / `ticket_board`).
- [x] No dependency on `cpe-server` or the app (ADR 0001 / CI guard); pure logic cargo-tested.

## Notes
Prereq: **CPE-851**.

## Work Log

## Resolution
Made the sidecar's UI a real, interactive Agent Board — reimplemented in the sidecar (no `cpe-server`/app
dependency; ADR 0001 / CI one-way guard holds):

- `src/board.rs` (new) — the Kanban model over the real files: `COLUMNS`, `folder_for_column` /
  `status_for_column`, frontmatter parsing → `Card {id,title,type,priority,tags,column}`, `set_status`
  (rewrite the `status:` line), `read_board(root)` (scan `Tickets/<col>/*.md`, id-less/unreadable skipped,
  column-then-id sorted), and `move_card(root, id, to)` (write the moved file with rewritten status, then
  remove the old — never lose a ticket). `nearest_project_root` walk-up. 6 tests.
- `src/ui.rs` — upgraded the loopback server into a tiny router: `GET /` → the Kanban page, `GET
  /api/cards` → JSON, `POST /api/move {id,to}` → move + refreshed cards (bad move → 400). The page is a
  self-contained HTML+JS board (a column per status, drag-and-drop → `POST /api/move`), theme-aware via CSS
  system colors. 2 tests (serve→page/cards/move round-trip through real files; valid HTML).
- `src/lib.rs` — `board_root()` resolves the project root (`CPE_BOARD_ROOT` env → nearest `Tickets/`
  ancestor of cwd → cwd); host-brokered `context` supplies it properly in CPE-853.
- `src/main.rs` — serves `board_root()` on Ready.
- `Cargo.toml` — added `serde` (derive) + `tempfile` (dev); still only the contract + serde, no app dep.

Verification (local, Windows): `cargo test` → **12 passed** (board + UI routing round-trip + protocol);
`cargo clippy --all-targets -D warnings` clean. The one-way guard holds. Host launch/open-from-explorer is
CPE-853; bundling so it appears in Settings is CPE-854.

## Work Log
- 2026-07-21 — Reimplemented the board model in the sidecar (contract-free), upgraded the UI server to a
  router with a real drag-to-move Kanban over `Tickets/`, and resolved the board root from env/cwd. 12
  tests + clippy clean. Closing.
