---
title: Agent Board
order: 6
category: Agent Deck
categoryOrder: 3
---

# Agent Board

The Agent Board is a **Kanban view over your tickets** — the `Tickets/` markdown files in a project
folder. It is not a separate task list; it reads and writes the same files the ticket workflow uses, so
the board and the command-line flow never drift.

Open it from **Agent Board** in the left sidebar.

## Its own window

The board can also live in **its own window**, separate from the file explorer — handy on a second
monitor while you browse files in the main window. Open it two ways:

- the **⧉** button in the board's title bar (pops the board out into a window), or
- the command palette → **Open Agent Board in a window**.

It's a **singleton**: there is only ever one board window — opening it again just focuses the one you
already have. The embedded in-board view still works too, so you can use whichever fits. The window
remembers its size and position between runs.

## Choosing a project

The board reads a project's `Tickets/` folder. When you open it inside a repo, it **finds the project
automatically** — it walks up from the current folder to the nearest one that has a `Tickets/` folder.
You can also click **📁 Project** to point it at any project folder; your choice is **remembered** for
next time. If no `Tickets/` folder is found, the board tells you so (and offers the picker) instead of
showing a blank panel.

## Columns and cards

- Each **column is a status folder** (Backlog, Doing, Review, Blocked, Deferred, Done); each **card is a
  ticket**.
- **Drag a card** to another column to change its status — the ticket file moves and its `status:` is
  updated.
- The bottom **status bar** shows per-column counts and the folder; drag the corner **grip** to resize.

## Dispatch and review

- **Dispatch** on a card moves it to Doing and opens an agent scoped to that ticket.
- Send a card to **Review** before Done.

## Filtering

Type in the **Filter cards** box to narrow the board by ticket **id, title, tag, type, or priority** —
across every lane and the Epics view at once. Press **Esc** to clear it. If nothing matches, the board
says so rather than showing empty columns.

## Epics view

Toggle **Epics** to see your epics as their own **kanban** — laid out just like the ticket board. Each
epic is a card in the column that matches its status:

- **Backlog** — proposed (dormant) epics, not yet started.
- **Doing** — activated epics that are in progress.
- **Done** — closed epics. Like the ticket board, older ones are archived behind a **+N archived** toggle.

Every epic card shows its **id**, **status**, and a **progress bar** (how many of its tickets are done).
**Click an epic card** to jump to the Board filtered to that epic's tickets. Your view choice (Board or
Epics) and the archived toggle are **remembered** between opens.
