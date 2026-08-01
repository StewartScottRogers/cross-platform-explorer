---
id: CPE-1210
title: "Backend (Windows, OS-gated): junction creation + New Link 'Junction' kind"
type: feature
component: Backend
priority: low
status: Backlog
tags: ready
created: 2026-08-01
epic: CPE-715
---

## Summary
Part of CPE-715. Windows directory junctions (the Windows dir-link). OS-gated — needs a reparse-point syscall
and the Windows CI runner for verification.

## Build
- `#[cfg(windows)] create_junction(target, link_path)` via reparse-point DeviceIoControl (or the `junction`
  crate — flag the dep for review). Tauri command + binding. Add "Junction" as a third kind in the New Link
  dialog (CPE-1207), shown only on Windows.

## Acceptance Criteria
- [ ] cargo test (Windows-gated) creates + resolves a junction; skipped elsewhere. **Attended cross-OS verify
      flagged** (Windows CI runner). clippy clean; `npm run check` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-715). OS-gated; after CPE-1207. Build+unit-test what's
  possible; Windows-runner verifies.
