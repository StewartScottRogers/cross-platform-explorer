---
id: CPE-739
title: "EPIC: Scriptable actions / user macros"
type: Task
status: Done
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

## Work Log

- 2026-07-23: Activated (Proposed → In Progress). First slice CPE-938 — the pure headless action-macro
  model (`crates/server/src/action_macro.rs`): `MacroStep`/`ActionMacro`/`PlannedOp` + `validate` + a
  filesystem-free `plan` expansion, std-only, 14 tests. Foundation the parameter-prompt UI, action library,
  and menu/hotkey/rule bindings will build on.

2026-07-25 (sprint) — **CPE-1033** added the action-macro persistence store
(`cpe-server::macro_store`: `save`/`list`/`load`/`delete`/`import`/`export` over a `macros.json` catalog
keyed by macro name, `ServerCtx`-based, tolerant-read, `HeadlessCtx`-tested), following the CPE-836 template
store. A user's macro library now survives restarts. Confirmed distinct from the in-memory
`macro_library` (CPE-951) — that's CRUD+ordering+validation, this is disk persistence. Independently
reviewed (redundancy explicitly checked) + UAT-passed (PR #350). Remaining: parameter-prompt UI, action
library surfacing, and menu/hotkey/watched-folder bindings (GUI/attended).

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** Parameter-prompt UI + menu/hotkey/watched-folder bindings unbuilt (only macro model + store).

2026-07-31 (sprint) — Re-activated; decomposed into CPE-1187..1191. **Macro backend phase landed**
(CPE-1187 + CPE-1188, one branch/PR): `crates/server/src/macro_run.rs` resolves `(ActionMacro, inputs)` into
a collision-safe, scope-checked, reversible op list (CPE-1187), and 9 thin Tauri commands
(`macro_save`/`list`/`load`/`delete`/`export`/`import`/`plan`/`run`/`undo`) now bridge that plan to the real
`rename_entry`/`move_exact`/tag-store/media-convert primitives, all-or-nothing with automatic rollback on a
mid-run failure (CPE-1188) — **the macro engine is reachable from the frontend for the first time**.
Bindings regenerated. CPE-1190's *model* half (an additive `{ask:label}` prompt-parameter token +
`plan_with_params`) rode along on the same branch; its prompt-UI half is still open in Backlog. Remaining
before DoD: CPE-1190's UI half, an action-library gallery, and menu/hotkey/watched-folder bindings (all
GUI/attended, per the 2026-07-30 review above).
