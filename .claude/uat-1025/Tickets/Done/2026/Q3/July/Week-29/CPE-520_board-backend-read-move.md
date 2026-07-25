---
id: CPE-520
title: "Agent Board — backend: read Tickets/ as cards + move a card between columns"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
epic: CPE-503
sprint: SPR-03
closed: 2026-07-16
estimate: 3-4h
created: 2026-07-16
---

## Summary
Foundation of the Agent Board ([[CPE-503]], wave 1). The board is backed by the **real `Tickets/`
folders** (activation decision). Provide the backend: read every `Tickets/{Backlog,Doing,Blocked,
Deferred,Done}/CPE-*.md` into **cards** (id, title, type, priority, tags, column) and **move a card**
between columns (move the file + sync its `status:` frontmatter). Pure frontmatter parsing + a safe move.

## Acceptance Criteria
- [x] A `board_cards(root)` command returns all tickets grouped by column (folder), each with id, title,
      type, priority, tags — parsed from frontmatter; a malformed file is skipped, never a hard failure.
- [x] A `board_move(root, id, to_column)` command moves the ticket file to the target folder AND updates
      its `status:` frontmatter to match; an unknown id/column is refused; never clobbers an existing file.
- [x] A pure, unit-tested frontmatter parser + folder↔status↔column mapping.
- [x] Reads are read-only; the move is the only writer and is atomic-ish (validate then move).
- [x] Tests for the parser + the column/status mapping (file-move exercised where feasible).

## Resolution
Added `src-tauri/src/ticket_board.rs` (new, pure, 6 tests) + two native Tauri commands in `lib.rs`.

- **Pure core:** `card_from(md, column)` parses a ticket's `---` frontmatter into a `Card`
  (id/title/type/priority/tags/epic/sprint/column) — tolerant (no id ⇒ skipped, no frontmatter ⇒ None);
  `parse_tags` handles `[a, b]` / quoted / empty; `folder_for_column` + `status_for_column` map the 5
  workflow columns ↔ folders ↔ the `status:` value; `set_status(md, s)` rewrites (or inserts) the status
  line, preserving the rest.
- **Commands (ungated — plain filesystem, keeps the sidecar boundary clean):** `board_cards(root)` reads
  every `<root>/Tickets/{Backlog,Doing,Blocked,Deferred,Done}/CPE-*.md` into cards (read-only, skips
  malformed); `board_move(root, id, to_column)` locates the `<id>_*.md` file, refuses an unknown
  id/column, **won't clobber** an existing destination, rewrites its status frontmatter, then moves the
  file. A move to the current column is a no-op.

Backed by the **real `Tickets/` files** (activation decision) so the board + CLI `/ticketing-*` share one
source of truth. Verified: `cargo clippy --all-targets` clean in **both** feature modes; 6 unit tests
(card parse, id-skip, tag parse, column/status mapping, status rewrite + insert). First ticket of SPR-03.

## Work Log
2026-07-16 — Picked up (SPR-03 wave 1; foundation of CPE-503). Estimate: 3-4h.
2026-07-16 — Built ticket_board.rs (pure frontmatter→Card + column/status mapping + set_status) with 6 tests, and board_cards/board_move commands (read-only list + safe status-rewrite-then-move). Ungated native commands. clippy clean both feature modes. All ACs met.

## Notes
Wave 1 of [[CPE-503]]. Backs the board UI (CPE-521). Real-folder backing per the activation decision.
