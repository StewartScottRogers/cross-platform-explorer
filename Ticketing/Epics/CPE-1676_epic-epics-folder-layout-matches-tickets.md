---
id: CPE-1676
title: "EPIC: Give the Epics queue the same status-folder layout as Tickets"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
created: 2026-08-12
closed:
---

> **Filed + activated 2026-08-12** by the user, mid-sprint, with screenshots of both folders side by side.
> Requested to be done next.

## Goal

`Ticketing/Tickets/` is organised into status folders — `Backlog/`, `Blocked/`, `Deferred/`, `Doing/`,
`Done/` — and **folder location is the authoritative status**. `Ticketing/Epics/` is a flat list of ~70
`CPE-NNN_epic-*.md` files whose status lives only in frontmatter, so you cannot see the state of the epic
queue by looking at it.

Make the Epics queue structurally identical to the Tickets queue: the same five folder names, with folder
location authoritative, so both queues read the same way and the same mental model applies to each.

## Why this is an epic and not a ticket

Folder location being authoritative means **every reader of that folder changes in lockstep or the board
lies**. The queue is read by at least seven independent consumers:

| Consumer | File |
|---|---|
| In-process ticket board | `crates/server/src/ticket_board.rs`, `src-tauri/src/lib.rs` |
| Standalone board sidecar | `sidecar/agent-board/src/board.rs`, `ui.rs` |
| Ticket MCP server | `sidecar/ai-console/src/swarm_mcp.rs` |
| `/ticketing-epic` skill | `.claude/commands/ticketing-epic.md` |
| `/ticketing-list`, `/ticketing-new`, `/ticketing-work` | `.claude/commands/*.md` |
| Project instructions | `CLAUDE.md` (the status table + the "always show Epics" rule) |
| Typed bindings | `src/lib/bindings.gen.ts` |

The board exists **twice** plus the MCP — a standing hazard on this project — and all three read the ticket
folders directly. A partial migration leaves a board that silently shows an empty or half-populated epic
queue, which is worse than the flat list it replaces.

## Status mapping

Epics use `Proposed` (dormant brief) and `In Progress` (activated), plus closed. Map onto the Tickets
folders rather than inventing a parallel vocabulary:

| Epic status | Folder |
|-------------|--------|
| `Proposed` — dormant brief, not yet decomposed | `Backlog/` |
| `In Progress` — activated | `Doing/` |
| Closed | `Done/` |
| Externally gated | `Blocked/` |
| Postponed by our choice | `Deferred/` |

`Blocked/` and `Deferred/` start empty; they exist so the two queues are genuinely the same shape and an
epic can be parked without inventing a folder later.

## Slices

1. **Move the files and make folder location authoritative.** Create the five folders, `git mv` every epic
   into the one matching its frontmatter `status:`, and keep the frontmatter field in sync (it is
   duplicative but every consumer currently reads it, so removing it is a separate decision).
2. **Update the readers, in lockstep.** Both boards, the MCP, and the four `/ticketing-*` skills. Whatever
   glob or path each uses must find epics at the new depth *and* derive status from the folder.
3. **Update `CLAUDE.md`** — the status table, the Epics description, and the "showing open tickets" rule
   that tells the assistant where to look.
4. **Guard it.** A test that fails if an epic file sits directly in `Ticketing/Epics/` (the old shape), and
   one that fails if a file's frontmatter `status:` disagrees with its folder — the same class of guard
   `sectionDocs.test.ts` provides for the docs registry.

## Acceptance criteria

- [ ] `Ticketing/Epics/` contains only the five status folders — no loose `.md` files.
- [ ] Every epic is in the folder matching its status, and its frontmatter agrees.
- [ ] Both board implementations and the ticket MCP list epics correctly, with the right status, **verified
      by running them** — not by reading the code.
- [ ] `/ticketing-epic list`, `/ticketing-list`, `/ticketing-new` and `/ticketing-work` all behave as before
      from the user's point of view.
- [ ] `CLAUDE.md`'s status table and epic rules describe the new layout.
- [ ] A guard test fails on a loose file at the old location, and another fails on a folder/frontmatter
      mismatch. Break each deliberately and watch it go red.
- [ ] `git log --follow` still works on a moved epic (use `git mv`, not delete-and-add).

## Notes

The user asked for this explicitly, with screenshots, and asked for it next. The screenshots show
`Ticketing/Epics/` as a flat file list against `Ticketing/Tickets/` as five folders — the contrast *is* the
requirement.

One thing to decide during slice 1 and record: whether `Done/` gets the same
`Done/YYYY/QN/Month/Week-NN/` nesting the Tickets queue uses. Tickets need it because there are thousands;
there are ~70 epics and few will ever close, so flat `Done/` is probably right — but say so deliberately
rather than by omission, because `/ticketing-organize` is what maintains that nesting and it will need to
know which shape to expect.
