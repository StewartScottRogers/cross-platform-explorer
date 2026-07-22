---
id: CPE-865
title: Auto-archive Done tickets into subfolders (on-close + session hook)
type: feature
component: Tooling
priority: medium
tags: ready
created: 2026-07-21
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
- [ ] `scripts/organize-done.mjs` reproduces the `/ticketing-organize` result idempotently; `npm run
      organize:done` works; unit-ish self-check on a temp tree.
- [ ] `/ticketing-organize` + `/ticketing-work` invoke the script (no duplicated prose algorithm).
- [ ] `SessionStart` hook runs it; leaves feature branches untouched (commits only on `main`).
- [ ] Running it when `Done/` is already organized is a no-op (no spurious moves/commits).

## Notes
- The ISO-week math must match GNU `date +%V` (the structure already on disk) so new tickets file into the
  same `Week-NN` folders.
