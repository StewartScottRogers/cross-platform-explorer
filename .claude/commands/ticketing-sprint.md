Manage cross-platform-explorer **sprints** — named, time-boxed sets of tickets to focus a batch of work.

`$ARGUMENTS` selects the subcommand: `list` · `new "<name>"` · `activate SPR-NN` · `close SPR-NN` ·
`assign CPE-NNN [SPR-NN]` · `remove CPE-NNN`. No argument ⇒ `list`.

---

## What a sprint is (and how it differs from an epic)

A **sprint** is a **time-boxed batch of tickets** worked together toward a near-term goal — the "what
are we doing *now/next*" grouping. It is **orthogonal to epics**: an epic is a *thematic* umbrella
decomposed just-in-time; a sprint is a *time-boxed* selection that can pull tickets from **any** epic
(or none). A single ticket may belong to both an epic (`epic:`) and a sprint (`sprint:`) at once.

Sprints live in **`Tickets/Sprints/`**, one markdown file per sprint, id **`SPR-NN`** (two digits,
sequential — `SPR-01`, `SPR-02`, …; a **separate** sequence from `CPE-NNN`). Membership is authoritative
via the **`sprint: SPR-NN`** frontmatter field on member tickets (so progress is countable by globbing);
the sprint file's `## Tickets` section is a human-readable mirror kept in step on assign/remove.

## Lifecycle

```
Planned                 activate              Active                     close (all done / time up)
(Sprints/,        ────────────────────▶  (Sprints/,             ──────────────────────────────▶  Done/
 status:            the current focus)      status: Active,                                        (status:
 Planned)                                   one at a time*)                                        Closed)
```

The folder (`Sprints/` vs `Done/`) is the sprint's status, mirrored in `status:`. *Convention: keep
**one** Active sprint at a time (the current focus); Planned sprints queue behind it.

## Sprint file shape

```markdown
---
id: SPR-NN
title: "Sprint N — <short name>"
status: Planned            # Planned | Active | Closed
start: YYYY-MM-DD
end: YYYY-MM-DD            # the time-box target
created: YYYY-MM-DD
closed:                    # set on close
---

## Goal
One or two lines: what this sprint aims to land.

## Tickets
- [ ] CPE-NNN — <title>   (mirrors the `sprint:` field; check off as they reach Done)
```

---

## `list` (default)

1. Glob `Tickets/Sprints/SPR-*.md`. Read frontmatter (id, title, status, start, end) + the Goal line.
2. For each sprint, count member progress: glob every ticket whose `sprint:` frontmatter equals this
   sprint's id, across `Backlog/ Doing/ Blocked/ Deferred/ Done/`, and report `X of Y tickets Done`.
3. Render (Active first, then Planned):

   | ID | Title | Status | Window | Goal | Progress |
   |----|-------|--------|--------|------|----------|

   (`Window` = `start → end`.) If `Sprints/` is empty, say "No sprints."
4. Then render the menu:

```
┌─ Sprint Actions ─────────────────────────┐
│  [1] New  [2] Activate  [3] Close        │
│  [4] Assign a ticket                     │
├───────────────────────────────────────────┤
│  [5] Dismiss                             │
└───────────────────────────────────────────┘
```

`[1] New` → ask a name (+ optional start/end), run `new`. `[2] Activate` → ask which id, run `activate`.
`[3] Close` → ask which id, run `close`. `[4] Assign` → ask a ticket id (+ sprint), run `assign`.

---

## `new "<name>"`

1. Next id: scan `Tickets/**/SPR-*.md` for the highest `SPR-NN`, add 1 (zero-pad to 2).
2. Create `Tickets/Sprints/SPR-NN_<slug>.md` with the shape above: `status: Planned`, `start`/`end`
   (default `end` = start + 2 weeks unless the user gives one), a Goal, and an empty `## Tickets` list.
3. Report the created sprint and offer to `activate` it or `assign` tickets.

## `activate SPR-NN`

1. Set the sprint's `status: Active`. If another sprint is already Active, say so and confirm the switch
   (convention: one Active at a time) before flipping the old one back to `Planned` (or leave both Active
   only if the user insists).
2. It stays in `Sprints/`. Report the active sprint + its tickets.

## `close SPR-NN`

1. List the sprint's member tickets (by `sprint:`), noting which are Done vs still open.
2. If open tickets remain, ask whether to (a) carry them to another sprint (`assign` them onward),
   (b) drop them from the sprint (`remove`), or (c) close anyway with a documented carry-over note.
3. Write a short Resolution (what landed), set `status: Closed` + `closed: YYYY-MM-DD`, and move the file
   `Sprints/ → Done/` (same Done/ depth-detection as `ticketing-work`). Member tickets keep their
   `sprint:` field as a historical record.

## `assign CPE-NNN [SPR-NN]`

1. Resolve the sprint: the given `SPR-NN`, else the single **Active** sprint (error if none/ambiguous).
2. Find ticket `CPE-NNN` in any folder. Add/append `sprint: SPR-NN` to its frontmatter (a ticket has at
   most one sprint — replacing a prior one moves it, and update both sprint files' `## Tickets` lists).
3. Add `- [ ] CPE-NNN — <title>` to the sprint's `## Tickets` (checked if the ticket is already Done).
4. Report the assignment.

## `remove CPE-NNN`

1. Delete the `sprint:` field from the ticket and drop its line from the sprint's `## Tickets` list.

---

## Rules (invariants)

- **`SPR-NN` is a separate id sequence** from `CPE-NNN`; sprints never take a CPE id.
- **Membership is the `sprint:` field** on tickets (authoritative + countable); the sprint's `## Tickets`
  list mirrors it — keep them in step.
- **Sprints are orthogonal to epics** — a ticket can carry both `epic:` and `sprint:`.
- **One Active sprint at a time** (convention) — the current focus; others are Planned.
- **A sprint never "works" tickets itself** — its members are ordinary tickets worked via
  `/ticketing-work`; the sprint is a lens over them.

---

## Menu Extension Point

This skill's menu follows `menu-render.md`. To add an option: add it to the rendered block, add its
handler above, and update the changelog in `menu-render.md`.
