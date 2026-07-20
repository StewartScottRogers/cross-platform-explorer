---
id: CPE-710
title: "EPIC: File attributes, permissions & timestamps editor"
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

## Work Log
2026-07-20 (nightshift, 02:00 MST) — Activated. Open questions resolved (autonomous): Windows = common
attribute toggles (hidden/read-only/system/archive) + take-ownership via `run_as_admin` (full ACL editor
deferred to a follow-up); recursive apply uses the transfer-queue conventions (GUI child); reversible =
chmod + attribute toggles + timestamps (store prior state for undo). POSIX permission model lands pure first.

## Child tickets
1. **CPE-784** — Pure POSIX permission model (`src/lib/permissions.ts`): mode ↔ symbolic `rwxr-xr-x` ↔
   octal, parse/format both ways, `describePermissions` + `setPermission` bit toggles. Unit-tested.
   **Foundation, headless.**
2. **CPE-785** — Backend commands: `set_permissions(path, mode)` (POSIX chmod), Windows attribute toggles,
   `set_file_times(path, …)` (async + spawn_blocking; cargo-tested). **Backend, CI-verified.**
3. **CPE-786** — Attributes/permissions/timestamps editor dialog: per-OS UI (POSIX mode grid vs. Windows
   attribute toggles), batch apply with a change preview, undo for reversible changes. **Attended GUI.**
   *(prereq: 784, 785)*
