Manage cross-platform-explorer **epics** — umbrella trackers that are decomposed *just-in-time*.

`$ARGUMENTS` selects the subcommand: `list` · `activate CPE-NNN` · `close CPE-NNN`.
No argument ⇒ `list`.

---

## What an epic is (and what it is NOT)

An epic is a headline goal too large for one unit of work. It lives in **`Tickets/Epics/`** — a queue
**separate from the Backlog** — and stays there as a one-page brief (goal, rough scope, open
questions) with **no child tickets and no research or building** until it is **activated**.

Decomposition is deliberately **just-in-time**: you do NOT plan, research, or sub-ticket an epic while
it sits dormant. Up-front breakdown rots as scope drifts, and a backlog full of speculative child
tickets hides what is actually workable. **Pulling an epic from the queue IS the decision to invest in
planning it** — and that is the only moment it gets decomposed.

An epic is never placed in `Doing/`, and `/ticketing-work` never builds one directly (it redirects
here). Only an epic's *children* are worked, as ordinary Backlog tickets.

## Lifecycle

```
Proposed              activate            Active                     all children Done + DoD
(Epics/,        ───────────────────▶  (Epics/,              ──────────────────────────────▶  Done/
 status:          research + decide +   status: In Progress,                                  (status:
 Proposed,        create children)      children in Backlog/)                                  Done)
 no children)
```

The folder (`Epics/` vs `Done/`) is the epic's status, mirrored in `status:`. A `Proposed` epic is
dormant; an `In Progress` epic has been activated and has children in flight.

---

## `list` (default)

1. Glob `Tickets/Epics/CPE-*.md`. Read frontmatter (id, title, status, tags) + the Summary's first line.
2. For each **Active** epic (`status: In Progress`), count child progress: glob every ticket whose
   `epic:` frontmatter equals this epic's id, across `Backlog/ Doing/ Blocked/ Deferred/ Done/`, and
   report `X of Y children Done`.
3. Render:

   | ID | Title | Status | Tags | Goal | Children |
   |----|-------|--------|------|------|----------|

   (`Children` blank for `Proposed` epics — they have none yet, by design.)
4. If `Epics/` is empty, say "No epics." Then render the menu:

```
┌─ Epic Actions ───────────────────────────┐
│  [1] Activate one  [2] Close one         │
├───────────────────────────────────────────┤
│  [3] Dismiss                             │
└───────────────────────────────────────────┘
```

`[1] Activate` → ask which id, then run `activate` below. `[2] Close` → ask which id, run `close`.

---

## `activate CPE-NNN` — the breakdown / grooming flow (the ONLY place an epic is decomposed)

Do this **only** when we have decided to pursue the epic now.

1. **Read** the epic brief fully — goal, rough scope, open questions.
2. **Research now** (not before): read the code, docs, and sibling features the epic touches. This is
   the first time any investigation happens for this epic.
3. **Resolve open questions.** For every product/UX/architecture decision the brief flags
   (`needs-decision`), ASK the user (use AskUserQuestion) and record the answer under a `## Decisions`
   section in the epic. Never guess a product call to unblock decomposition.
4. **Decompose into child tickets.** For each slice of work, create a Backlog ticket exactly as
   `ticketing-new` would (next sequential `CPE-NNN`, frontmatter, estimate, disposition tag,
   acceptance criteria) with one addition: an `epic: CPE-NNN` frontmatter field linking it back to
   this epic. Order them and note prerequisites (`needs-prereq`, naming the sibling id) where they
   exist. Record the list under `## Child tickets` in the epic (id + title + one-line scope).
5. **Set the epic Active.** Update the epic frontmatter `status: In Progress` (it stays in `Epics/`).
   Append a Work Log entry summarising the decisions made and the children created.
6. **Hand off.** The children are now ordinary Backlog work. Offer to `/ticketing-list` (to sequence
   them) or `/ticketing-work` the first child. Do **not** work the epic itself.

If activation research reveals the epic is no longer worth doing, say so and offer to close it as
`Won't Fix` (Resolution explaining why) rather than leaving a dead brief in the queue.

---

## `close CPE-NNN`

1. **Verify children.** Glob every ticket with `epic: CPE-NNN`. Every one must be in `Done/`. If any
   sit in `Backlog/ Doing/ Blocked/ Deferred/`, list them and **stop** — an epic does not close over
   open children. (Exception: with the user's explicit ok, close with a documented **carve-out** for a
   deliberately-deferred child — record which child and why in the epic. Never silently.)
2. **Check the epic-level Definition of Done** gates in the brief; tick the ones that hold.
3. **Write the epic Resolution** (what shipped, which children delivered it, any carve-outs), set
   `status: Done` + `closed: YYYY-MM-DD`, and move the file `Epics/ → Done/` (use the same Done/
   depth-detection as `ticketing-work`).
4. Report the closeout: children delivered, gates met, anything carved out.

---

## Rules (invariants)

- **No decomposition while `Proposed`.** Research, planning, and sub-ticketing happen *only* at
  `activate`. A dormant epic is a brief, nothing more.
- **Epics are never worked directly.** They are never in `Doing/`; `/ticketing-work` redirects an epic
  target here. Only children are built.
- **Every child carries `epic: CPE-NNN`** so progress is countable and the epic closes exactly when
  its children (and DoD) do.
- Epics use the `epic` disposition tag and live in `Epics/`; they never sit in `Backlog/`.

---

## Menu Extension Point

This skill's menu follows `menu-render.md`. To add an option: add it to the rendered block, add its
handler above, and update the changelog in `menu-render.md`.
