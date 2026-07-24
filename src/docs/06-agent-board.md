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
**Click an epic card** to open its **details** (below); its popup has a **View tickets →** button that
filters the Board to that epic's tickets (including closed/archived ones). Your view choice (Board or
Epics) and the archived toggle are **remembered** between opens.

## Card details

**Click any card** (a ticket or an epic) to open a read-only **details popup** showing the full ticket —
its frontmatter fields (type, priority, status, tags, epic, sprint, folder) and the rendered body
(Summary, Acceptance Criteria, Work Log, …).

- **Resize** it by dragging the hatched **thumb** in the bottom-right corner; the size is remembered.
- The **status bar** along the bottom shows the ticket's folder location and field/line counts.
- Click the **⧉ pop-out** button to open the card in its **own window**, separate from the board — handy
  for keeping a ticket open while you work elsewhere. (The copy-id, Dispatch, and drag controls on the
  card still work — clicking those doesn't open the popup.)

## Directives — talk to an agent

The card popup has a **Send directive** box: type an instruction (optionally naming a target agent,
default `any`) and press **Send ▸**. It writes a structured, machine-readable entry into the ticket under
a **`## Agent Directives`** heading:

```
### ▸ open · to `any` · 2026-07-24T01:48:22Z
Summarize the risks in this ticket
```

**The ticket file is the wire.** Because directives live in the ticket's `.md`, any agent that can see the
repo — local, remote, in CI, or one you don't control — can read `open` directives, do the work, append a
reply, and flip the status to `done`. No open ports, no live connection required.

### The `ticket-mcp` server (MCP)

For **live, tool-using agents**, a ready-made **MCP server** exposes these directives over the Model
Context Protocol. Point any MCP client at:

```
ticket-mcp <path-to-your-repo>
```

It offers two tools:

- **`directives.list`** — every `open` directive across the project's tickets (ticket id, `when`
  timestamp, target, and the instruction text).
- **`directives.reply`** — append a reply to a directive by its `when` timestamp, and optionally resolve
  it (`open` → `done`).

So an external agent can survey the board's directives and answer them, with every exchange recorded in
the ticket itself.
