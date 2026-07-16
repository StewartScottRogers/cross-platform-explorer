---
id: CPE-482
title: "Show the Windows wait cursor for all perceptible wait conditions in the forms"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
---

## Summary
Whenever the app is doing something that takes a **perceptible** amount of time, the pointer should
change to the OS **wait/busy cursor** (Windows' spinning ring / "AppStarting" `progress` cursor) so
the user gets immediate feedback that the app is working, instead of a UI that looks frozen or
unresponsive. This applies across **both** form surfaces: the main explorer (Svelte, `src/`) and the
AI Console launcher (`sidecar/ai-console/src/launcher.html`).

## Approach
A small, central "busy" mechanism rather than ad-hoc per-call CSS:
- A reference-counted busy tracker (increment on start, decrement on finish/error) that toggles a
  `busy` class on `document.body` → `body.busy, body.busy * { cursor: progress !important; }`
  (`progress` = the Windows arrow-with-spinner; use `wait` for fully-blocking operations).
- A helper that wraps an async operation: `await withBusy(() => invoke(...))` — always clears on both
  resolve and reject (never leave the cursor stuck).
- **Debounce the show by ~150 ms** so instant operations don't flash the cursor (avoid flicker); show
  immediately once the threshold is crossed, clear as soon as the last in-flight op finishes.

## Perceptible wait conditions to cover
**Explorer (`src/`):**
- Directory listing / navigation (`list_dir`), especially network drives + large folders
- Search in files (content search), Find duplicates
- Archive extraction, checksum computation
- Copy / move / delete (Recycle Bin) operations
- Large-file preview loading
- Repositories: browse / clone / pull / push (forge)
- Check for updates; disk-usage computation

**AI Console launcher (`launcher.html`):**
- Launching an agent (spawn), Close-all
- Fetching the model list / snapshot
- Install / update / uninstall an agent (and aggregate ops)
- Provider key verification; catalog update / rollback

## Acceptance Criteria
- [ ] A reusable busy helper/store exists in each surface (explorer + launcher) and is used by the
      async operations above.
- [ ] The pointer shows the Windows wait/`progress` cursor during each perceptible operation and
      reverts the instant it completes — including on error/rejection (no stuck cursor).
- [ ] Operations under the debounce threshold (~150 ms) do NOT flash the cursor.
- [ ] Nested/concurrent operations are handled (reference-counted; the cursor clears only when the
      last one finishes).
- [ ] Tests: the busy tracker's counting + threshold logic is unit-tested; a launcher jsdom test
      asserts the body `busy` class toggles around a wrapped async call.

## Notes
"Windows Cursor" here = the OS busy pointer (CSS `cursor: progress` / `wait`), applied app-wide for
perceptible waits. Keep it cheap and central so new async actions can opt in with one wrapper call.
Requested by the user 2026-07-15.
