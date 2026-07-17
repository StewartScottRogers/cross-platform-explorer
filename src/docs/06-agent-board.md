---
title: Agent Board
order: 6
---

# Agent Board

The Agent Board is a **Kanban view over your tickets** — the `Tickets/` markdown files in the current
folder. It is not a separate task list; it reads and writes the same files the ticket workflow uses, so
the board and the command-line flow never drift.

Open it from **Agent Board** in the left sidebar.

## Columns and cards

- Each **column is a status folder** (Backlog, Doing, Review, Blocked, Deferred, Done); each **card is a
  ticket**.
- **Drag a card** to another column to change its status — the ticket file moves and its `status:` is
  updated.
- The bottom **status bar** shows per-column counts and the folder; drag the corner **grip** to resize.

## Dispatch and review

- **Dispatch** on a card moves it to Doing and opens an agent scoped to that ticket.
- Send a card to **Review** before Done.

## Epics view

Toggle **Epics** to organize by epic: pick an epic on the left and see its tickets with **to-do on top
and done on the bottom**. The Done column keeps only recent items; older ones are archived (shown behind
a **+N archived** toggle).
