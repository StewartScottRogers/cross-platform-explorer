---
id: CPE-406
title: Sidebar shows a used/free space bar under each drive
type: feature
priority: medium
estimate: S
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [ui, explorer]
depends-on: CPE-403
---

## Problem / value
Windows Explorer shows a thin usage bar under each drive so you can see at a glance which disks are
full. This app lists drives with no capacity signal. Reuses the CPE-403 disk_space command.

## Scope
- App fetches disk_space for each drive once (non-blocking, on mount) into a usage map.
- Sidebar renders a thin used/total bar (+ "N free") under each drive row when data is present;
  the bar turns amber/red as it fills. Additive: no data ⇒ no bar, sidebar unchanged.

## Acceptance
- [x] Each drive shows a usage bar reflecting used/total; low free space reads as amber/red
- [x] Pure percentage/severity helper unit-tested; no bar when data absent (off means off)
- [x] Non-blocking — a slow/failed probe never delays or breaks the sidebar
