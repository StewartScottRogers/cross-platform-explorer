---
id: CPE-734
title: "EPIC: Watched-folder rules (when/then automation)"
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
