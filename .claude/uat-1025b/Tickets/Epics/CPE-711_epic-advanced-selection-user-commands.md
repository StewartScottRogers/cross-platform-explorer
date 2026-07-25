---
id: CPE-711
title: "EPIC: Advanced selection & user-defined commands"
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

## Work Log
2026-07-20 (nightshift, 01:27 MST) — Activated. Open questions resolved (autonomous): selection criteria
**reuse the CPE-774 `Condition` model** (ext/glob/size/date/isDir — already covers pattern/type/date/size)
rather than a parallel matcher; template grammar `{path}/{name}/{dir}/{ext}/{stem}`, **per-item expansion**
(one invocation per selected entry) with a joined-list option; user commands surface in **toolbar + context
menu + palette**; external commands **confirm before running** (safety), wired in the GUI child. Two pure
cores land first.

## Child tickets
1. **CPE-780** — Pure selection-criteria engine (`src/lib/selectMatch.ts`): `selectMatching(entries,
   condition, now)` → indices (reuses CPE-774 `matchesCondition`) + `sameExtensionAs(entries, seed)`.
   Unit-tested. **Foundation, headless.**
2. **CPE-781** — Pure command-template runner (`src/lib/cmdTemplate.ts`): `expandTemplate(tpl, entry)`
   substituting `{path}/{name}/{dir}/{ext}/{stem}` + `expandForSelection(tpl, entries, mode)` (per-item vs
   joined). Unit-tested. **Headless.** *(independent of 780)*
3. **CPE-782** — "Select by…" dialog wiring `selectMatching` into the selection; selection survives
   sort/filter/refresh. **GUI.** *(prereq: 780)*
4. **CPE-783** — User-defined commands: bind templated commands (CPE-781) to toolbar/context-menu/palette,
   run over the selection via a backend exec command, **confirm before shelling out**. **GUI + backend.**
   *(prereq: 781)*
