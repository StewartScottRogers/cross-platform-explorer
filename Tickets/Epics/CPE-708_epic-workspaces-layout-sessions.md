---
id: CPE-708
title: "EPIC: Workspaces & layout sessions"
type: Task
status: In Progress
priority: Medium
component: Frontend
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Named, savable workspaces that capture the full window state — open tabs, folders, sort/view per tab,
pane sizes, filters, selected smart folders, dual-pane layout — restored in one click, with auto-restore
on launch.

## Why
Power users juggle project contexts ("Editing", "Taxes", "Downloads triage"). Making "reopen my
three-folder setup" a keystroke builds on existing tabs/columns/settings persistence.

## Rough scope (areas, not child tickets)
- A serializable session model covering tabs, paths, sort/view, filters, pane sizes, layout.
- A workspace switcher UI (save / rename / switch / delete).
- Launch-time restore that degrades gracefully when paths have moved or drives are absent.
- Reuse existing tab + settings persistence rather than a parallel store.

## Open questions (resolve at activation)
- Interaction with dual-pane ([[CPE-617]]) layout state.
- Auto-save the current workspace vs. explicit save only.
- Handling missing paths on restore (skip / prompt / placeholder).

## Definition of Done
- A user can save the current window state as a named workspace and switch between workspaces in one click.
- Auto-restore reopens the last session on launch and tolerates moved/missing paths.
- No change to default single-tab startup behaviour when workspaces are unused.

## Work Log
2026-07-20 (autonomous) — Activated. Open questions resolved: **explicit named workspaces** for v1 (auto-save
of last session is a separate restore child); **skip missing paths** on restore (graceful degrade, keep the
valid tabs); dual-pane layout field reserved but not required (CPE-617 unlanded). Reuse the existing tab +
settings persistence rather than a parallel store. Pure model lands first.

## Child tickets
1. **CPE-787** — Pure workspace model (`src/lib/workspaces.ts`): `Workspace`/`WorkspaceTab` types, tolerant
   parse/serialize, CRUD (add/rename/remove/update), and `pruneMissing(ws, exists)` to drop moved/missing
   tabs on restore. Unit-tested. **Foundation, headless.**
2. **CPE-788** — Workspace switcher UI: save the current tabs/views as a named workspace, switch / rename /
   delete, apply on select. **GUI.** *(prereq: 787)*
3. **CPE-789** — Launch-time auto-restore of the last session, tolerating moved/missing paths; no change to
   default single-tab startup. **GUI/integration.** *(prereq: 787)*
