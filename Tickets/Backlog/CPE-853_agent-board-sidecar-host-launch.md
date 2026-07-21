---
id: CPE-853
title: Agent Board sidecar — host launch + open-from-explorer
type: feature
component: Multiple
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-21
epic: CPE-850
estimate: 3-4h
---

## Summary
Third child of CPE-850. Launch + frame the `agent-board` sidecar from the explorer, mirroring how the AI
Console opens: the host starts the sidecar (registry entry → supervisor), and a launcher (toolbar/menu)
opens a window framing the sidecar's announced loopback UI. The existing in-process board window (CPE-841)
becomes a shortcut into this, or is repointed at the sidecar UI.

## Acceptance Criteria
- [ ] A launcher opens the Agent Board sidecar's UI (host starts the sidecar, frames `ui:<url>`).
- [ ] Lifecycle handled like other sidecars (start/stop, restart, reaping); consent gate for its
      capabilities.
- [ ] GUI-verified: launching shows the board served from the sidecar process.

## Notes
Prereq: **CPE-851**. **GUI-verified — attended.**

## Work Log
