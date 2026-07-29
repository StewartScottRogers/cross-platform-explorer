---
id: CPE-865
title: Auto-archive Done tickets into subfolders (on-close + session hook)
type: feature
component: Tooling
priority: medium
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
Keep `Tickets/Done/` from piling up without anyone remembering to run `/ticketing-organize`. Make the
existing date-subfolder archiving (`Done/YYYY/QN/MonthName/Week-NN/`, 50-per-dir threshold) run
**automatically**. User picked **Both** triggers (2026-07-21).

## Design
1. **`scripts/organize-done.mjs`** — one committed, idempotent Node script (Node is always present;
   cross-platform; no shell dependency) implementing the archive algorithm (subdivide any Done dir over
   the threshold by each ticket's `closed:` date; only moves files, never edits; skips undated; stops at
   week depth). Single source of truth. Add `npm run organize:done`.
2. **`/ticketing-organize`** skill → invoke the script instead of prose (manual/bulk path).
3. **`/ticketing-work`** skill → run the script right after moving a ticket into `Done/`, folding the
   archive moves into the close commit (**on-close** trigger).
4. **`.claude/settings.json` `SessionStart` hook** → run the script once per session as a **universal
   safety net** (catches tickets closed by hand or from the desktop/Cowork app). Scope any auto-commit to
   when the current branch is `main`, so it never pollutes a feature branch.

## Depends on
- **CPE-864** — the app must be archive-aware first, or automating the nesting keeps hiding Done tickets
  from the Agent Board. Sequence CPE-864 before relying on this.

## Acceptance Criteria
- [x] `scripts/organize-done.mjs` implements the algorithm (Node; idempotent; only moves, never edits);
      `npm run organize:done` runs it. Verified: on the already-organized tree it reports `moved=0` with 0
      working-tree changes, and its `Week-NN` matches the existing structure (28/29/30) — i.e. its ISO-week
      math matches GNU `date +%V`, so new tickets file into the same folders.
- [x] `/ticketing-organize` + `/ticketing-work` now invoke the script (single source of truth; the prose
      depth-math was removed from ticketing-work).
- [x] `.claude/settings.json` `SessionStart` hook runs `node scripts/organize-done.mjs --commit`; the
      `--commit` path only commits when the current branch is `main`, leaving feature branches untouched.
- [x] Running it when `Done/` is already organized is a no-op (no moves; no commit).

## Notes
- The ISO-week math must match GNU `date +%V` (the structure already on disk) so new tickets file into the
  same `Week-NN` folders.
