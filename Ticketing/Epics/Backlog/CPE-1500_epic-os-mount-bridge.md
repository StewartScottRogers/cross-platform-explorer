---
id: CPE-1500
title: "EPIC: Network F4 — OS-mount bridge ('Mount as drive', the hybrid arm)"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic]
epic: CPE-716
created: 2026-08-08
closed:
---

> **Network Filesharing program — the OS-mount arm (extends CPE-716 drive bay; complements CPE-616's in-app
> VFS). Foundation epic F4.** Filed 2026-08-08 (sprint PM, Network research). Dormant.

## Why (the hybrid escape hatch + the safe fallback for hard protocols)
The recommended model is **hybrid**: in-app VFS is the default ("Connect"); **OS-mount is the optional
"Mount as drive"** for protocols the OS mounts natively (SMB, NFS) and for anything with no in-app client yet.
Once mounted, CPE browses it as an ordinary **local path via `LocalProvider`**, and `net_share.rs` already
enumerates it into Drives/Shared. This is also the realistic **v1 path for SMB on Linux/macOS** (see CPE-1504).

## Scope
- Per-OS mount/unmount commands: Windows `net use` / `WNetAddConnection2`; macOS `mount_smbfs` / NetFS; Linux
  `mount.cifs` / `gio mount`. Elevation handling where required (no launch-time consent — [[avoid-modal-permission-popups]]).
- "Mount as drive" / "Unmount" per-connection actions (menu from CPE-1498); mounted share shows via existing
  enumeration.
- Split-rule: default in-app VFS; expose OS-mount when the user wants the share visible to OTHER apps or as a
  persistent drive. One connection profile can drive both.

## Effort / deps / fit
M–L (per-OS + elevation). Backend-heavy per-OS + small frontend action. Deps: CPE-1498 (menu). Independent of
CPE-1499 (a different path to the same goal). Opt-in, off by default. Reconciles CPE-716's open question #1
(how it sequences with CPE-616).

## Test target — real QNAP TS-133 NAS (available 2026-08-10)
The user's QNAP TS-133 (QTS: SMB 445 + NFS 111/2049) is the concrete device to validate "Mount as drive"
against on every OS: Windows `net use \\<qnap>\<share>` / `WNetAddConnection2`, macOS `mount_smbfs`, Linux
`mount.cifs` / `mount -t nfs` / `gio mount`. This is also the realistic **v1 SMB path** for Linux/macOS
(CPE-1504) — so a single QNAP share exercises both this OS-mount arm and the SMB/NFS protocol epics end-to-end.
