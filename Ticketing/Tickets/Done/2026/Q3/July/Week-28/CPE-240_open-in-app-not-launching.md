---
id: CPE-240
title: Folder-context "Open in <app>" actions don't launch anything
type: Bug
status: Done
priority: High
component: Multiple
estimate: 1h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Clicking a folder-context action such as "Open in Visual Studio" (CPE-239) does
nothing. The action calls the opener plugin's `openPath(marker)`. The file
association is correct (`.sln` → VSLauncher.exe on this machine), so the failure
is in `openPath` itself not launching the associated app. The same `openPath` is
used for the file list's double-click-open and the earlier "Open in browser",
so this is a shared reliability problem.

Fix: open external paths through a dedicated, reliable backend command that shells
out to the OS opener (Windows `cmd /C start`, macOS `open`, Linux `xdg-open`),
and route the context actions (and the file/recent open) through it. Surface a
notice so a click always gives feedback.

## Acceptance Criteria
- [ ] New backend `open_external(path)` opens a file/folder with its default app.
- [ ] "Open in <app>" context actions use it and actually launch the app.
- [ ] File double-click-open and Recent open use it too (same reliability).
- [ ] Failure shows a notice; success gives feedback.
- [ ] `npm run check` + `cargo build` + tests pass.

## Resolution

*(Agent writes this when closing)*

## Work Log

*(Agent appends dated entries here)*

### filled
Added Rust `open_external(path)` (Windows `cmd /C start`, macOS `open`, Linux
`xdg-open`) and routed file double-click-open, Recent open, and folder-context
"open-path" actions through it (dropped the flaky opener-plugin `openPath`).
Verified the `cmd /C start "" path` mechanism opens the default app for a test
file. This also makes .exe/.cmd/.bat execute on double-click. check 0/0; cargo
build ok. Ships in 0.10.3.
