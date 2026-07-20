---
id: CPE-734
title: "EPIC: Watched-folder rules (when/then automation)"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
A rule engine where the user defines triggers ("a file matching `*.pdf` larger than 5 MB lands in
Downloads") and actions ("move to ~/Docs/Invoices, tag `invoice`, rename by pattern"), automating the
repetitive sort-and-file work that fills a Downloads folder.

## Why
Automating "when X lands in Y, do Z" is the single highest-leverage productivity feature for everyday users
and reuses the app's existing move/rename/tag primitives and the FS watcher.

## Rough scope (areas, not child tickets)
- A background folder watcher (Rust notify) scoped to user-chosen folders.
- A rule model + editor UI (trigger conditions + ordered action pipeline).
- An action pipeline reusing existing move/rename/tag/copy primitives.
- Dry-run/preview + an activity log so rules never surprise the user.

## Open questions (resolve at activation)
- Trigger vocabulary (pattern/size/age/type) and reuse of the smart-folder predicate model.
- Safety: undo/quarantine for rule actions; loop/oscillation prevention.
- Runs only while the app is open, or a background service? (delete-test implications).

## Definition of Done
- Users define trigger->action rules on watched folders with a dry-run preview.
- Matching files are processed through the action pipeline with a visible activity log.
- Rules are opt-in and add no cost when none are defined; actions are reversible where possible.

## Work Log
2026-07-20 (autonomous) — Activated. Open questions resolved: triggers **reuse the CPE-774 `Condition`
model** (pattern/size/age/type); actions are an ordered pipeline (move/copy/tag/rename) with rename using
**CPE-781 templates**; **first-match** rule precedence (one rule handles a landed file, avoiding conflicting
moves); **dry-run = the pure planner** produces the planned actions without executing; **runs while the app
is open** v1 (background service deferred, delete-test); reversible via the existing undo. Pure planner first.

## Child tickets
1. **CPE-793** — Pure rule model + planner (`src/lib/watchRules.ts`): `WatchRule { when: Condition,
   actions: Action[] }`, `planForEntry(entry, rules, now)` → first matching rule's resolved actions (rename
   via CPE-781), CRUD + tolerant parse/serialize. Unit-tested. **Foundation, headless.**
2. **CPE-794** — Backend folder watcher (notify) + action executor reusing move/copy/tag primitives +
   activity log. **Backend/integration.** *(prereq: 793)*
3. **CPE-795** — Rule editor UI + dry-run preview + activity log. **GUI.** *(prereq: 793, 794)*
