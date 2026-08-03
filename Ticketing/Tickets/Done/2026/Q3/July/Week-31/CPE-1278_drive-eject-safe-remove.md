---
id: CPE-1278
title: "Drive eject / safe-remove for removable drives"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-716
---

## Summary
Epic CPE-716: drive listing + usage bars already ship in the sidebar (CPE-406, `list_drives`/`drive_type`). Add
SAFE eject / remove for REMOVABLE drives — enumerate removable volumes + an eject command + a sidebar/drive-view
eject affordance. Safety-critical: never eject the system/fixed drive; report in-use failures clearly.

## Build
- Backend (cpe-server or src-tauri where the OS calls live, mirroring drive_type): identify REMOVABLE drives
  (Windows `GetDriveType == DRIVE_REMOVABLE`); an `eject_drive(path)` that on Windows does the safe-remove sequence
  via `DeviceIoControl` on the volume handle: `FSCTL_LOCK_VOLUME` → `FSCTL_DISMOUNT_VOLUME` → `IOCTL_STORAGE_EJECT_MEDIA`
  (unlock on failure). Degrade on macOS/Linux (`diskutil eject` / `udisksctl` / umount) or honest not-yet-supported note.
  NEVER operate on a fixed/system drive — reject with a clear error. Report "drive in use / files open" failures clearly.
- Surface removable-ness so the UI can show an eject control ONLY on removable drives (extend the drive list/Place
  with an is_removable/ejectable flag, or a per-drive query). Regen bindings if a struct changes.
- UI: an eject button/affordance on removable drives in the sidebar Drives section (and/or drive view); on click →
  eject_drive → toast success ("Safe to remove") or the failure reason; refresh the drive list.
- async + spawn_blocking; capability entries if needed.

## Acceptance criteria
- Removable drives are identified; eject runs the safe dismount+eject; fixed/system drives are NEVER ejectable
  (rejected). In-use failure surfaces a clear message, not a crash.
- cargo build/test/clippy clean (all modes); no new dep if avoidable (use windows-sys/winapi already present, or the
  existing OS-call pattern); CPE-1271 guard + bindings drift green; unit-test the removable-detection + the guard that
  refuses fixed/system drives (the eject syscall itself is attended/hardware).

## Notes
Attended verify: plug in a USB, eject it from the sidebar, confirm Windows reports it safe to remove; confirm the
system drive shows no eject control + can't be ejected. Part of the CPE-716 drive-bay epic (listing already shipped).

## Work Log
- 2026-08-03 — Removable-drive eject/safe-remove: drive_ejectable query + eject_drive (Windows lock→dismount→eject via DeviceIoControl, unlock/close on all paths, in-use error, never panics). Safety: eject_guard default-deny (removable-only), guard-before-syscall via `?` (no bypass path), 5 hardware-free tests incl. refuses-fixed-never-calls-syscall + refuses-system-drive. UI eject affordance on removable rows + context menu, toast. Reused windows crate (3 features, no new dep). cargo test 114, vitest 1928, bindings zero-drift. Security reviewer APPROVE. Merged #577. NOTE (cosmetic, follow-up): the eject glyph renders as an outline, not the solid eject symbol — worth a polish.
