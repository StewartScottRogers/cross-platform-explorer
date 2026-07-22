---
id: CPE-403
title: Status bar shows free disk space for the current drive
type: feature
priority: medium
estimate: S
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [ui, explorer]
---

## Problem / value
A classic file-explorer affordance is missing: the status bar never shows how much space is free on
the current drive. Users routinely check this before copying large files. `list_drives` doesn't
compute it, so it's a genuine gap.

## Scope
- Backend `disk_space(path)` command returning { free, total } bytes (cross-platform via the fs4
  crate — statvfs / GetDiskFreeSpaceExW under the hood).
- Frontend: on navigating into a real folder, fetch it and show "X free of Y" in the status bar;
  degrade silently on error; hidden for Home / inside archives.

## Assumption logged (Nightshift — user asleep)
Added a small, well-established base-app dependency (fs4, "High" reputation) for a core feature,
consistent with the app's existing dependency posture. CI's cross-OS build is the gate; watched.

## Acceptance
- [x] disk_space returns sensible { free, total } for a real path (Rust test on the temp dir)
- [x] Status bar shows free/total for the current drive; absent on Home/archive/error
- [x] Pure formatter unit-tested
