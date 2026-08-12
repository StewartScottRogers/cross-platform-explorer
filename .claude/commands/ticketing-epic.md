Manage cross-platform-explorer **epics** — umbrella trackers that are decomposed *just-in-time*.

`$ARGUMENTS` selects the subcommand: `list` · `activate CPE-NNN` · `close CPE-NNN`.
No argument ⇒ `list`.

---

## What an epic is (and what it is NOT)

An epic is a headline goal too large for one unit of work. It lives in **`Ticketing/Epics/`** — a queue
**separate from `Ticketing/Tickets/`** — and stays there as a one-page brief (goal, rough scope, open
questions) with **no child tickets and no research or building** until it is **activated**.

Since CPE-1676 the Epics queue has the **same five status folders** as the Tickets queue —
`Backlog/ Doing/ Blocked/ Deferred/ Done/` — and, exactly as there, **folder location is the
authoritative status**, mirrored in the file's `status:` frontmatter:

| Epics folder | Epic `status:` | Meaning |
|--------------|----------------|---------|
| `Epics/Backlog/`  | `Proposed`    | Dormant brief. No children, no research. |
| `Epics/Doing/`    | `In Progress` | Activated; children in `Tickets/Backlog/` with `epic: CPE-NNN`. |
| `Epics/Blocked/`  | `Blocked`     | Gated by something **external**. Normally empty. |
| `Epics/Deferred/` | `Deferred`    | Parked by **our** choice / an internal prereq. Normally empty. |
| `Epics/Done/`     | `Done`        | Closed. **Flat** — no dated `YYYY/QN/…` nesting. |

An epic's `Epics/Doing/` is *not* the tickets queue's one-at-a-time `Doing/`: several epics can be
activated at once. `Ticketing/Epics/` itself must hold **only** those five folders — a loose `.md`
there fails the guard test `src/lib/epicsQueueLayout.test.ts`, as does a `status:` that disagrees
with its folder.

Decomposition is deliberately **just-in-time**: you do NOT plan, research, or sub-ticket an epic while
it sits dormant. Up-front breakdown rots as scope drifts, and a backlog full of speculative child
tickets hides what is actually workable. **Pulling an epic from the queue IS the decision to invest in
planning it** — and that is the only moment it gets decomposed.

An epic is never placed in the **tickets** queue's `Tickets/Doing/`, and `/ticketing-work` never builds
one directly (it redirects here). Only an epic's *children* are worked, as ordinary Backlog tickets.

## Lifecycle

```
Proposed              activate            Active                     all children Done + DoD
(Epics/Backlog/, ──────────────────▶  (Epics/Doing/,        ──────────────────────────────▶  Epics/Done/
 status:          research + decide +   status: In Progress,                                  (status:
 Proposed,        create children)      children in                                            Done,
 no children)                           Tickets/Backlog/)                                      closed:)
```

The folder is the epic's status, mirrored in `status:`. A `Proposed` epic is dormant; an
`In Progress` epic has been activated and has children in flight.

---

## `list` (default)

1. Glob `Ticketing/Epics/*/CPE-*.md` — all five status folders. The **folder** gives the status
   (`Backlog/`→Proposed, `Doing/`→In Progress, …); read the rest of the frontmatter (id, title, tags)
   + the Summary's first line. Ignore each folder's `wiki.md` explainer.
2. For each **Active** epic (in `Epics/Doing/`), count child progress: glob every ticket whose
   `epic:` frontmatter equals this epic's id, across `Tickets/{Backlog,Doing,Blocked,Deferred,Done}/`
   (Done recursively — it is dated-nested), and report `X of Y children Done`.
3. Render:

   | ID | Title | Status | Tags | Goal | Children |
   |----|-------|--------|------|------|----------|

   (`Children` blank for `Proposed` epics — they have none yet, by design.)
4. If the five folders hold no `CPE-*.md`, say "No epics." Then render the menu:

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
5. **Set the epic Active.** `git mv` the file `Epics/Backlog/ → Epics/Doing/` **and** update its
   frontmatter to `status: In Progress` — the two must always agree (guard test). Append a Work Log
   entry summarising the decisions made and the children created.
6. **Hand off.** The children are now ordinary Backlog work. Offer to `/ticketing-list` (to sequence
   them) or `/ticketing-work` the first child. Do **not** work the epic itself.

If activation research reveals the epic is no longer worth doing, say so and offer to close it as
`Won't Fix` (Resolution explaining why) rather than leaving a dead brief in the queue.

---

## `close CPE-NNN`

1. **Verify children.** Glob every ticket with `epic: CPE-NNN`. Every one must be in `Tickets/Done/`.
   If any sit in `Tickets/{Backlog,Doing,Blocked,Deferred}/`, list them and **stop** — an epic does not
   close over open children. (Exception: with the user's explicit ok, close with a documented
   **carve-out** for a deliberately-deferred child — record which child and why in the epic. Never
   silently.)
2. **Check the epic-level Definition of Done** gates in the brief; tick the ones that hold.
3. **Write the epic Resolution** (what shipped, which children delivered it, any carve-outs), set
   `status: Done` + `closed: YYYY-MM-DD`, and `git mv` the file `Epics/Doing/ → Epics/Done/`.
   `Epics/Done/` is **flat** — do NOT apply the dated `YYYY/QN/Month/Week-NN/` nesting
   `ticketing-work` uses for `Tickets/Done/`; there are ~70 epics in total, so depth buys nothing and
   every reader is simpler without it.
4. Report the closeout: children delivered, gates met, anything carved out.

---

## Rules (invariants)

- **No decomposition while `Proposed`.** Research, planning, and sub-ticketing happen *only* at
  `activate`. A dormant epic is a brief, nothing more.
- **Epics are never worked directly.** They never sit in the tickets queue's `Tickets/Doing/`;
  `/ticketing-work` redirects an epic target here. Only children are built.
- **Every child carries `epic: CPE-NNN`** so progress is countable and the epic closes exactly when
  its children (and DoD) do.
- Epics use the `epic` disposition tag and live in `Ticketing/Epics/`; they never sit in
  `Ticketing/Tickets/`.
- **Folder = status, always.** Every move rewrites `status:` in the same edit, and nothing is ever
  left loose in `Ticketing/Epics/` itself. Both guards live in `src/lib/epicsQueueLayout.test.ts`, so
  breaking either fails CI — and both board implementations plus the ticket MCP read these folders
  directly, so a drift would make them lie rather than error.

---

## Menu Extension Point

This skill's menu follows `menu-render.md`. To add an option: add it to the rendered block, add its
handler above, and update the changelog in `menu-render.md`.
