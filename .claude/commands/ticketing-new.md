Create a new cross-platform-explorer ticket interactively.

## Auto-Intercept Trigger

Do not wait to be told "make a ticket." When the user asks for a **unit of project work** — a feature,
a bug/defect fix (including small live fixes), a refactor, a behavior change, or any multi-file edit —
intercept it and run this flow **transparently**: announce it in one line ("This is ticket-worthy —
filing CPE-NNN and working it"), create the ticket here, then `/ticketing-work` it through to Done.

Do NOT intercept (just do the thing): answering questions, analysis, running build / commit / push /
git ops, cutting or publishing a release, managing tickets or the skill system, trivial one-liners
being iterated live, or anything the user says to "just do." If it is genuinely borderline, ask first.
See "When to Auto-File a Ticket" in `Tickets/wiki.md` for the authoritative boundaries.

**Epic exception:** if the request is epic-sized (a Mega-Feature / many-child effort), do NOT work it
to Done. File it as a brief in the **Epics queue** and stop — see Step 3c. Epics are decomposed later,
just-in-time, via `/ticketing-epic activate`.

## Step 1 — Find the Next ID

Scan **all** ticket folders recursively — `Tickets/Epics/`, `Backlog/`, `Doing/`, `Blocked/`,
`Deferred/`, `Done/` — for every CPE-NNN_*.md file. Find the highest NNN. New ID = that number + 1,
zero-padded to three digits (e.g. CPE-022). (IDs are shared across all queues, epics included.)

## Step 2 — Gather Details

Ask the user for all of the following in a single message:

  Title:     (sentence case, concise)
  Type:      Bug | Defect | Task | Feature | Test
  Priority:  Low | Medium | High | Critical
  Component: Frontend | Backend | Updater | CI | Packaging | Docs | Multiple
  Summary:   (one or two sentences — enough context for someone reading cold)

For Bug and Defect tickets also ask:
  Environment:        (OS, app version, Node/Rust version if relevant)
  Steps to Reproduce: (numbered, from clean state)
  Expected Behavior:
  Actual Behavior:

## Step 3 — Generate an Estimate

Before writing the file, estimate how long it will take YOU (the agent) to complete this ticket.
Base the estimate on: type, component, number of likely acceptance criteria, and complexity implied by the summary.

Use these time bands:
  15m   — trivial: single-line change, config value, obvious fix with no side effects
  30m   — simple: a few lines in one file, clear acceptance criteria, no cross-layer impact
  1h    — moderate: one component, a handful of criteria, some investigation needed
  1-2h  — involved: multiple criteria, frontend + backend touch points
  2-3h  — complex: multiple files across one layer, non-trivial investigation or algorithm
  3-4h  — large: spans multiple components (Frontend + Backend + CI), error handling
  4h+   — major: architectural change, many criteria, high regression risk

State the estimate and your reasoning in one sentence before proceeding.

## Step 3b — Assign a Disposition Tag

Pick the ticket's `tags:` from the controlled vocabulary in `Tickets/wiki.md` ("Disposition Tags").
Exactly one primary tag (`ready` · `big-design` · `needs-decision` · `needs-prereq` · `epic` ·
`resource-blocked`), plus any qualifiers. Most freshly-filed, actionable work is `ready`. If it's
`resource-blocked`, add a qualifier (e.g. `needs-macos-linux`) so the listing shows what's needed.
Never coin a tag that isn't in the wiki — add it there first.

## Step 3c — Epic fork (STOP if this is epic-sized)

If the item is too big for one unit of work — a Mega-Feature, or something that will clearly need
many child tickets — it is an **epic**, and epics take a different path. Do **not** file it into
`Backlog/`, and do **not** decompose it now. Instead:

1. Create it in **`Tickets/Epics/`** with `status: Proposed` and `tags: [epic]`, as a one-page
   **brief only**: the goal (Summary), a rough scope, and any open `## Open questions` — **no child
   tickets, no research, no acceptance-criteria breakdown of sub-work**. (An epic-level Definition of
   Done is fine; per-slice detail is not.)
2. Report: "Filed epic CPE-NNN in the Epics queue — decompose it later with
   `/ticketing-epic activate CPE-NNN`."
3. **Skip the rest of this skill.** An epic is never worked directly; its research, planning, and
   sub-ticketing happen only at activation (see the `ticketing-epic` skill). Do not offer "Work it now".

Everything below (Steps 4–5) is for a normal, single-unit ticket.

## Step 4 — Create the File

Derive a kebab-case filename slug from the title (3-8 words, lowercase, hyphens).

Write Tickets/Backlog/CPE-NNN_slug.md using the structure below.
Delete the Environment / Steps / Expected / Actual sections if type is not Bug or Defect.

```markdown
---
id: CPE-NNN
title: {Title}
type: {Type}
status: Open
priority: {Priority}
component: {Component}
tags: [{tags from Step 3b}]
estimate: {estimate from Step 3}
created: YYYY-MM-DD
closed:
---

## Summary

{Summary}

## Environment

{Environment — bugs/defects only}

## Steps to Reproduce

{Steps — bugs/defects only}

## Expected Behavior

{Expected — bugs/defects only}

## Actual Behavior

{Actual — bugs/defects only}

## Acceptance Criteria

- [ ] ...

## Resolution

*(Do not fill this in — the agent writes this section when closing the ticket)*

## Work Log

*(Do not fill this in — the agent appends dated entries here throughout the work)*

## Notes

*(Optional)*
```

After writing the file, ask the user to fill in the Acceptance Criteria
(or offer to draft them based on the Summary if the user wants help).

## Step 5 — Confirm and Menu

Report: "Created CPE-NNN — '{Title}' (estimate: {estimate})."

Then render the action menu following the rules in menu-render.md. HORIZONTAL layout:

```
┌─ What Next? ────────────────────────────┐
│  [1] Work it now  [2] File another      │
├─────────────────────────────────────────┤
│  [3] Dismiss                            │
└─────────────────────────────────────────┘
```

### [1] Work it now
Invoke /ticketing-work CPE-NNN for the ticket just created.

### [2] File another
Restart this skill from Step 1.

### [3] Dismiss
Exit without action.
