---
id: CPE-786
title: Attributes / permissions / timestamps editor dialog
type: feature
status: Done
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-710
estimate: 3-4h
---

## Summary
The editor UI for epic CPE-710: a per-OS dialog — POSIX mode grid (owner/group/other rwx + octal, via
CPE-784) vs. Windows attribute toggles — that batch-applies changes (with a preview) via CPE-785 and
registers undo for reversible edits. Launched from Properties / context menu.

## Acceptance Criteria
- [x] Single-file and multi-select batch edits of permissions/attributes/timestamps, with a change preview.
      *(single-file + **multi-select batch** now apply edited flags/mode/**modified-timestamp** to every
      target; a **change preview** lists what will change before Apply.)*
- [x] Each OS shows its native model; reversible changes are undoable; no regression to read-only Properties.
      *(native per-OS model (Windows flag grid / POSIX octal+symbolic); **undo** re-applies each target's
      captured baseline; read-only Properties untouched.)*
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

## Update — batch + timestamps + undo landed (2026-07-20), all ACs met
Extended `AttributesDialog.svelte` from single-file to the full editor:
- **Multi-select batch**: takes a `targets[]`; reads each target's baseline (`read_attributes` + its modified
  time); the shared controls seed from the first target, and **only the fields the user edits** apply to
  **every** target.
- **Timestamp editing**: a `datetime-local` "Modified" input (`set_file_times`). New pure helper
  `src/lib/datetimeInput.ts` (`msToLocalInput`/`localInputToMs`, 4 unit tests) converts epoch-ms ↔ the
  control's local wall-clock value.
- **Change preview**: a live "Will change:" chip row of the edited fields, shown before Apply (Apply is
  disabled until something changes).
- **Undo**: each target's baseline (flags/mode + modified time) is captured up front; after Apply the dialog
  shows **Undo** (re-applies every baseline) + **Close**.
- **Ordering fix (found in verification)**: Windows rejects `set_file_times` on a **read-only** file
  ("Access is denied", os error 5). Apply/undo now clear read-only first, write the timestamp, then set
  read-only **last** — so read-only + timestamp in one apply (and editing an already-read-only file's time)
  both succeed.

**Verified:** `datetimeInput` (4 tests) + `AttributesDialog.test.ts` (3 component tests: batch heading/note/
timestamp render + both targets read; a changed field previews and applies to **both** targets; Apply
disabled until a change). Full suite **898 green**; `npm run check` clean. **GUI-verified against the live
backend (CDP):** `set_file_times` set a file's mtime to the exact target; and the dialog's **batch
apply→undo command sequence** run on two real files applied Read-only + a new modified time to **both**
(`ro:true, mod:target` on each) and **undo restored both** (`restoredRo:true, restoredMod:true`) — this run
is what surfaced the read-only/timestamp ordering bug, now fixed and re-verified clean.

## Resolution (batch + timestamps + undo — complete)
The attributes editor now does single-file **and** multi-select batch edits of Windows attributes / POSIX
mode / the modified timestamp, with a change preview and in-dialog undo (per-target baselines re-applied).
Reuses the existing CPE-785 write commands + `set_file_times`; the only new backend-adjacent care is the
read-only-before-timestamp ordering on Windows. Files: `AttributesDialog.svelte` (batch/timestamp/preview/
undo + ordering), `datetimeInput.ts` (+test), `AttributesDialog.test.ts` (new), `App.svelte`
(`attrTargets` multi-select + `applied` refreshes without closing so Undo stays available). All ACs met.
CPE-786 → Done.
