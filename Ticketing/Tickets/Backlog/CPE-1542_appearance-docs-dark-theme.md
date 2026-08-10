---
id: CPE-1542
title: "Dark theme: update the Appearance docs page for the System/Light/Dark control"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-1493
created: 2026-08-09
---
## Context
Per [[maintain-in-app-docs-library]] (CPE-579), every feature that changes a documented user-facing
control must update its doc page. `src/docs/35-appearance.md` (shipped by CPE-1537) currently documents
the Appearance control as **System/Light only**, explicitly says *"Right now both options look
identical... a Dark option will appear alongside Light... [in] what's coming"* — this ticket is that
promised update, once CPE-1539/1540/1541 land the real thing. The `appearance` section is **already**
registered in `src/lib/sectionDocs.ts` (mapped to `35-appearance.md`) — no registry change needed, this
is a content-only edit to an existing page.

## Scope
- Rewrite `src/docs/35-appearance.md`'s "The Theme control" section: three options (System / Light /
  Dark), System now follows the OS live (including a running OS theme change, no restart needed), Dark
  can be picked explicitly regardless of the OS.
- Remove or repurpose the "What's coming" section — the thing it was describing has shipped; either drop
  it or replace it with a short forward-pointer to the rest of the theme program (native accent color,
  window materials, high-contrast — the sibling epics CPE-1494/1495/1496) if that reads better than
  deleting it outright.
- No change to `src/lib/sectionDocs.ts` — the `appearance` → `35-appearance.md` mapping already exists.

## How
- Pure Markdown content edit. Cross-check the final copy against whatever CPE-1541 actually shipped as
  the `.note` line under the Settings select, so the docs page and the in-app hint don't contradict each
  other.

## Verify
`npm run check` (the `sectionDocs.test.ts` guard still passes since no slug/section changed); read the
rendered page once via the in-app docs viewer if convenient, but no GUI verification is required to land
a Markdown content edit — this ticket doesn't touch any component or test file.

## Notes
**Conflict surface:** `src/docs/35-appearance.md` only. No `sectionDocs.ts`, `App.svelte`, or `app.css`
edits. **Dispatch order:** last — after CPE-1541 (so the docs describe what actually shipped, not a
plan). Lowest-risk ticket in the batch; fine to park at the back of the queue.
