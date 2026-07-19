---
id: CPE-711
title: "EPIC: Advanced selection & user-defined commands"
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
A power-selection toolkit (select by pattern/type/date/size, invert, expand-to-same-extension, persist
selection across sort) paired with user-defined commands: custom toolbar buttons and context-menu entries
that run a template over the current selection (`{path}`, `{name}`, `{dir}`, `{ext}`).

## Why
This is the "make the explorer mine" layer that defines Directory Opus: precise selection plus scriptable,
templated actions bound to the UI. Reuses the selection model and command-palette infrastructure.

## Rough scope (areas, not child tickets)
- A selection-criteria engine (pattern/type/date/size, invert, extend) with a "Select by..." dialog.
- Selection stability across sort/filter/refresh.
- A command-template runner (`{path}` / `{name}` / `{dir}` / `{ext}`), multi-item expansion.
- Toolbar + context-menu customization UI to bind user commands.

## Open questions (resolve at activation)
- Safety/sandboxing of user commands that shell out; confirmation UX.
- Template grammar scope (per-item loop vs. one invocation with a list).
- Where user commands surface (toolbar, context menu, command palette — all?).

## Definition of Done
- Users can select by rich criteria and the selection survives sort/filter.
- Users can define templated commands and bind them to the toolbar/context menu, run over the selection.
- Commands prompt before running external processes; no regression to built-in actions.
