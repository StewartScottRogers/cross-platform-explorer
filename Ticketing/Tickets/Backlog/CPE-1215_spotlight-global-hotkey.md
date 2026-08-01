---
id: CPE-1215
title: "Spotlight global hotkey (tauri-plugin-global-shortcut) + Settings control"
type: feature
component: Multiple
priority: medium
status: Backlog
tags: ready
created: 2026-08-01
epic: CPE-704
---

## Summary
Part of CPE-704. A global OS hotkey that opens spotlight even when the window is hidden. OS-gated.

## Build
- Add `tauri-plugin-global-shortcut` dep + init; add the capability entry to `src-tauri/capabilities/default.json`
  (`global-shortcut:allow-register`/`allow-unregister`) or the register is denied at runtime. Register a default
  chord that emits a `spotlight:open` event. **Enable/disable + chord live in Settings — NEVER a launch-time
  modal** ([[avoid-modal-permission-popups]]). Unregister cleanly on disable (no background cost when off).

## Acceptance Criteria
- [ ] Builds; clippy clean both modes; capability present; the setting persists + toggling register/unregisters.
- [ ] **OS-gated:** the hotkey firing while the window is hidden is attended-verified (flagged, not headless).

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-704). Depends on CPE-1214's event/open. OS-gated.
