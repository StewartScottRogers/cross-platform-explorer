---
id: CPE-1019
title: Windows shell-registration plan (pure model)
type: feature
component: Backend
priority: medium
tags: ready
epic: CPE-712
created: 2026-07-24
closed: 2026-07-24
status: Done
---

## Summary
Second slice of the shell-citizen epic (CPE-712), building on CPE-945's applicability model. A **pure,
headless** function in `cpe_server::shell_menu` that turns the "Open in Cross-Platform Explorer" integration
into an explicit list of Windows registry operations — with **no registry I/O** (that glue is CPE-1020).
Expressing the registration as data is what makes the reversibility guarantee testable.

`windows_shell_plan(exe_path, app_name) -> WinShellPlan` where:
- `RegEntry { key, value_name, value }` — one registry value to write (default value ⇒ empty `value_name`).
- `WinShellPlan { install: Vec<RegEntry>, remove: Vec<String> }` — values to write, plus the full key paths
  to delete on uninstall.

Registers under **HKCU** (`Software\Classes\...`) so **no elevation** is needed, per the epic's open
question on privilege. Covers the three surfaces that matter for a file explorer:
- on-folder (`Directory\shell\CPE`), command uses `"%1"`,
- folder-background (`Directory\Background\shell\CPE`), command uses `"%V"`,
- on-drive (`Drive\shell\CPE`), command uses `"%1"`.
Each `...\shell\CPE` carries a label + `Icon` value pointing at the exe, and a `...\command` subkey whose
default value is the quoted exe + the path placeholder.

## Acceptance Criteria
- [ ] `windows_shell_plan` returns install entries for the on-folder, folder-background, and on-drive verbs
      with correct `HKCU\Software\Classes\...\shell\CPE\command` key paths and `"exe" "%1"` / `"exe" "%V"`
      command strings; exe path with spaces is quoted.
- [ ] **Reversibility invariant** (unit-tested): every root `...\shell\CPE` key touched by an install entry
      appears in `remove`, so uninstall leaves no residue.
- [ ] `app_name` drives the visible label; icon value references `exe_path`.
- [ ] Pure — no `winreg`/no I/O; clippy clean both feature modes; ≥4 unit tests.

## Work Log
- 2026-07-24 (PM take-on) — Filed as the next CPE-712 slice after CPE-945. Chosen headless-first so the
  registry contract is fully unit-tested before any HKCU writes land (CPE-1020 applies it).
- 2026-07-24 — **Done.** Added `windows_shell_plan` + `RegEntry`/`WinShellPlan` to `cpe_server::shell_menu`
  (pure, no `winreg`). Registers on-folder/`%1`, folder-background/`%V`, on-drive/`%1` under
  `HKCU\Software\Classes\…\shell\CPE`; quoted exe; label from `app_name`; `Icon` = exe. **Checks:** 8/8
  module tests pass (4 new); clippy clean both feature modes. **Independent review:** inline
  correctness/reuse/altitude/conventions pass — no findings. **UAT:** ran the plan and confirmed the emitted
  registry layout is the canonical "Open in X" verb shape, with a remove-set covering every installed root
  (no residue). All acceptance criteria met. Next: CPE-1020 applies the plan via HKCU glue.
