---
id: CPE-786
title: Attributes / permissions / timestamps editor dialog
type: feature
status: Deferred
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-710
estimate: 3-4h
---

## Summary
The editor UI for epic CPE-710: a per-OS dialog — POSIX mode grid (owner/group/other rwx + octal, via
CPE-784) vs. Windows attribute toggles — that batch-applies changes (with a preview) via CPE-785 and
registers undo for reversible edits. Launched from Properties / context menu.

## Acceptance Criteria
- [~] Single-file and multi-select batch edits of permissions/attributes/timestamps, with a change preview.
      *(**single-file** edit of Windows attrs / POSIX mode done & verified; **multi-select batch + timestamps** are a follow-up.)*
- [~] Each OS shows its native model; reversible changes are undoable; no regression to read-only Properties.
      *(native per-OS model done (Windows flag grid / POSIX octal+symbolic); no Properties regression; **undo** is a follow-up.)*
- [x] check + suite green; GUI-verified.

## Notes
Prereq: CPE-784, CPE-785. Attended GUI. Recursive apply + full Windows ACL editing are follow-ups.

## Resolution (single-file editor shipped + verified; batch/timestamps/undo deferred)
Added the missing backend read `read_attributes(path) -> FileAttributes` (Windows readonly/hidden/system/
archive from `GetFileAttributesW`; POSIX readonly + octal mode) — the write side (`set_readonly` /
`set_file_attribute` / `set_permissions`, CPE-785) already existed. Built
`src/lib/components/AttributesDialog.svelte`: reads current values, then on Windows shows a
readonly/hidden/system/archive checkbox grid (applies via `set_readonly` / `set_file_attribute` per changed
flag), on POSIX an octal-mode input with a live symbolic preview (`modeToSymbolic`, applies via
`set_permissions` + `octalToMode`). Opened from the command palette ("Attributes…") for the single selected
entry; refreshes the listing on apply. `palette.attributes` in all 12 locales.

**GUI-verified in the running dev app (CDP):** selected a test file → its `read_attributes` showed
`readonly:false, archive:true` → opened the editor (Windows grid: Read-only/Hidden/System/Archive) →
toggled **Read-only on → Apply** → the file became **`-r--r--r--` on disk** and a backend re-read confirmed
`readonly: true`. Test file cleaned up. `npm run check` clean; permissions/i18n suites green. Backend has a
cargo test (readonly read round-trips a `set_readonly` toggle); clippy clean both feature modes.

Deferred tail: **multi-select batch** apply, **timestamp** editing (`set_file_times` exists), and **undo** —
plus a change preview for batch. Single-file attribute/permission editing is complete.
