---
id: CPE-708
title: "EPIC: Workspaces & layout sessions"
type: Task
status: Proposed
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
