---
id: CPE-740
title: "EPIC: Folder templates & scaffolding"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Save any folder's structure (subfolders, placeholder files, naming conventions) as a reusable template and
stamp it out on demand — new-project, new-client, new-shoot layouts in one click, with token substitution
(date, name, counter).

## Why
Kills the repetitive "create the same six subfolders again" chore for anyone with a recurring project
structure — developers, photographers, accountants. Small, self-contained, high everyday value.

## Rough scope (areas, not child tickets)
- Template capture from an existing folder (structure + placeholder files).
- A token/substitution engine (`{date}`, `{name}`, `{counter}`, ...).
- A template gallery (manage / import / export).
- A "New from template..." flow in the file view / context menu.

## Open questions (resolve at activation)
- Template storage/format and sharing (import/export).
- Token vocabulary and where substitution applies (folder names, file names, file contents?).
- Placeholder file contents (empty vs. templated boilerplate).

## Definition of Done
- Users can capture a folder as a template and stamp it out with token substitution.
- A template gallery manages templates with import/export.
- "New from template..." creates the structure in one action; no cost when unused.
