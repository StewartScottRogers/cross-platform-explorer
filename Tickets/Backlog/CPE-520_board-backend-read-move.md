---
id: CPE-520
title: "Agent Board — backend: read Tickets/ as cards + move a card between columns"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [ready]
epic: CPE-503
sprint: SPR-03
estimate: 3-4h
created: 2026-07-16
---

## Summary
Foundation of the Agent Board ([[CPE-503]], wave 1). The board is backed by the **real `Tickets/`
folders** (activation decision). Provide the backend: read every `Tickets/{Backlog,Doing,Blocked,
Deferred,Done}/CPE-*.md` into **cards** (id, title, type, priority, tags, column) and **move a card**
between columns (move the file + sync its `status:` frontmatter). Pure frontmatter parsing + a safe move.

## Acceptance Criteria
- [ ] A `board_cards(root)` command returns all tickets grouped by column (folder), each with id, title,
      type, priority, tags — parsed from frontmatter; a malformed file is skipped, never a hard failure.
- [ ] A `board_move(root, id, to_column)` command moves the ticket file to the target folder AND updates
      its `status:` frontmatter to match; an unknown id/column is refused; never clobbers an existing file.
- [ ] A pure, unit-tested frontmatter parser + folder↔status↔column mapping.
- [ ] Reads are read-only; the move is the only writer and is atomic-ish (validate then move).
- [ ] Tests for the parser + the column/status mapping (file-move exercised where feasible).

## Notes
Wave 1 of [[CPE-503]]. Backs the board UI (CPE-521). Real-folder backing per the activation decision.
