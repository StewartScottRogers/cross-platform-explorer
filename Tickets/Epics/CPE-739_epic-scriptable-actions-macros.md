---
id: CPE-739
title: "EPIC: Scriptable actions / user macros"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Let users capture or author reusable multi-step file operations (a sequence of rename/move/tag/convert
steps, optionally parameterized) and bind them to a menu item, hotkey, or watched-folder rule — turning
one-off manual sequences into repeatable, shareable one-clicks.

## Why
The automation capstone: once selection, rules, and user commands exist, letting users compose and share
multi-step actions is what makes CPE deeply customizable without shell scripting.

## Rough scope (areas, not child tickets)
- A safe action model (recorded-macro and/or a small sandboxed DSL) over existing op primitives.
- A parameter-prompt UI for parameterized actions.
- An action library with import/export (share actions between users).
- Binding to menu items, hotkeys, and watched-folder rules ([[CPE-734]]).

## Open questions (resolve at activation)
- Recorded macro vs. authored DSL vs. both; sandbox boundaries.
- Full undo support across a multi-step action.
- Overlap with user commands ([[CPE-711]]) — is this the multi-step superset?

## Definition of Done
- Users can create parameterized multi-step actions and bind them to menus/hotkeys/rules.
- Actions run over existing primitives with undo support and can be imported/exported.
- No action can run without user confirmation where it shells out or is destructive.
