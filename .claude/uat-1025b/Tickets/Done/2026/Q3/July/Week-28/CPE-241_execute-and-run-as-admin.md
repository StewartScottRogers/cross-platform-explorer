---
id: CPE-241
title: Execute executable files (double-click) + right-click Execute / Execute as admin
type: Feature
status: Done
priority: High
component: Multiple
estimate: 1-2h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

`.exe`, `.cmd`, and `.bat` (and other executables) should run when double-clicked.
CPE-240 fixed the underlying open path (double-click now shells out via
`open_external`, which executes them). This ticket adds the explicit right-click
actions: **Execute** and **Execute as administrator** for executable file types.

## Acceptance Criteria
- [ ] Double-clicking an executable runs it (delivered by CPE-240 — verify).
- [ ] Right-click on an executable file shows **Execute** and **Execute as admin**.
- [ ] Execute = run normally; Execute as admin = elevated (UAC prompt on Windows).
- [ ] The two items appear only for executable types (.exe/.cmd/.bat/.msi/.ps1/.com).
- [ ] Non-executables don't show them.
- [ ] `npm run check` + `cargo build` + tests pass.

## Notes
Needs a backend `run_as_admin(path)` (Windows: Start-Process -Verb RunAs; UAC).
ContextMenu gains the two items gated on an `executableSelected` flag. Relates to
CPE-240 (open_external) and CPE-047 (executable icon).

### filled
Double-click executes executables via CPE-240's open_external. Added right-click
Execute + Execute as administrator (ContextMenu, gated on executableSelected =
one selected file with an executable extension). Backend run_as_admin uses
Start-Process -Verb RunAs (UAC) on Windows. isExecutable() in filetypes.ts
(exe/cmd/bat/msi/com/ps1/scr/vbs). check 0/0; cargo build ok; 237 tests. Ships in 0.10.3.
