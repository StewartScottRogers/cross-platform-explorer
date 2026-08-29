---
id: CPE-716
title: "EPIC: Drive bay — removable & network volume manager"
type: Task
status: In Progress
priority: Low
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Detect, mount, safely eject/unmount, and remember removable and network volumes — USB media, disk images,
and SMB/NFS shares — with "map network drive / connect to server" dialogs.

## Why
Handling removable and network storage well (with safe eject) is a basic file-manager duty the app doesn't
yet cover. Extends the existing drive listing into an actionable volume manager.

## Rough scope (areas, not child tickets)
- Extend `list_drives` / `disk_space` with mount-state per volume.
- Eject/unmount commands per OS (Windows eject API, macOS `diskutil`, Linux `udisks`/`umount`).
- Removable-media arrival/removal notifications.
- "Map network drive" / "connect to server" dialogs (SMB/NFS), remembering connections.

## Open questions (resolve at activation)
- Overlap/sequencing with the remote & cloud filesystems epic ([[CPE-616]]) for network shares.
- Privilege requirements for mount/eject per OS.
- How mounted network shares appear in the sidebar (Connections vs. Drives).

## Definition of Done
- Removable and network volumes are detected with mount state and can be safely ejected/unmounted.
- Users can map/connect a network share and reconnect a remembered one.
- Media insert/remove updates the sidebar without a manual refresh.

## Work Log
2026-07-20 (nightshift) — Activated. Grep-first: `list_drives` enumerates drives but does NOT classify
type. First safe increment: a `drive_type` backend command (Windows `GetDriveTypeW`; unix best-effort).
Mount/eject + network-share management are heavier follow-ups.

## Child tickets
1. **CPE-805** — Backend `drive_type(path)` → fixed / removable / network / cdrom / ram / unknown
   (Windows `GetDriveTypeW`; unix returns "fixed"/"unknown" for now). cargo-tested. **Foundation, backend.**
2. **CPE-806** — Drive badges in the sidebar + a removable/network section, using `drive_type`. **GUI.** *(prereq: 805)*
3. **CPE-807** — Eject/unmount + network-share connect. **Backend + GUI.** *(prereq: 805)*

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** Eject/unmount + network-share connect + sidebar badges unbuilt (only drive_type classification).

## Closeout audit 2026-08-29 - KEEP OPEN

All 7 children Done. (The epic's own planned CPE-806/807 were never filed; those IDs went to unrelated tickets and the work landed under CPE-1278/1280/1024 instead.) Windows yes / Linux classification-only / macOS no.

**Shipped:** drive classification (`GetDriveTypeW` on Windows, `/proc/mounts` + `/sys/block/*/removable` on Linux); safe eject on Windows with an exhaustively-tested pure `eject_guard` that only lets `"removable"` through, and a LOCK -> DISMOUNT -> EJECT sequence; live insert/remove via `driveWatch.ts` polling with no manual refresh; network shares detected, parsed per-platform and remembered.

**THE GAP - on Linux and macOS no removable volume is ever listed.** `list_drives_impl` pushes a single `"/"` File System entry on every non-Windows platform, so `/media/...` and `/Volumes/...` never appear. Detection, badges, eject and live-arrival are therefore **Windows-only in practice**, and macOS never classifies anything as removable at all (its `drive_type_impl` is a stub returning `"fixed"`).

Second gap: **no unmount/disconnect for a network volume on any platform** - `eject_guard` explicitly refuses `"network"` and nothing else offers it. No drive-letter mapping either; `net_share.rs`'s own header calls the connect-to-server dialog and mount glue "future".
