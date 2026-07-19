---
id: CPE-710
title: "EPIC: File attributes, permissions & timestamps editor"
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
Direct editing of the things a real file manager must touch: Unix permissions/ownership (chmod/chown) and
Windows ACLs/attributes (hidden/read-only/system/archive), plus created/modified/accessed timestamps
("touch"), with multi-select batch apply and undo.

## Why
Properties is read-mostly today (CPE-035). Editing the platform's real permission and attribute model —
respecting each OS's conventions rather than a lowest-common-denominator abstraction — fills a concrete gap.

## Rough scope (areas, not child tickets)
- Cross-platform Rust commands: POSIX mode/owner/group, Windows attributes + ACLs (via existing run_as_admin), timestamps.
- A batch-safe editor dialog with a preview of changes and recursive apply.
- An "effective access" summary on Windows; clear per-OS UI.
- Undo integration for reversible attribute/time changes.

## Open questions (resolve at activation)
- How far into Windows ACL editing to go (full ACL editor vs. common toggles + take-ownership).
- Recursive apply safety and progress via the transfer-queue conventions.
- Which timestamp changes are reversible for undo.

## Definition of Done
- Permissions/ownership/attributes and timestamps can be edited on a single file and batch-applied.
- Each OS shows its native model (POSIX mode vs. ACLs/attributes) with a clear, safe UI.
- Reversible changes are undoable; recursive applies show progress and can be cancelled.
