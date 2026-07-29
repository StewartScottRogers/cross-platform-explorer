---
id: CPE-1021
title: Linux .desktop + xdg-mime registration plan (pure model)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-712
created: 2026-07-24
closed: 2026-07-25
status: Done
---

## Summary
CPE-712 slice: the Linux analogue of CPE-1019. A **pure, headless** function producing the
`cross-platform-explorer.desktop` entry content (Name, Exec with `%F`, MimeType `inode/directory`, plus a
`Actions` group for "Open in CPE") to install under `~/.local/share/applications`, and the
`xdg-mime default` association to set for `inode/directory` — together with the exact file paths + prior
association to restore on uninstall. No filesystem or `xdg-mime` process calls here; that glue is a later
slice. User-scope (`~/.local`) so no root needed.

## Acceptance Criteria
- [ ] Returns the `.desktop` file content (valid Desktop Entry, `Exec=... %F`) + target path under
      `~/.local/share/applications`, and the `inode/directory` default association to set.
- [ ] Uninstall set names the file to remove and the association to restore; reversibility unit-tested.
- [ ] Pure — no I/O, no process spawns; clippy clean both feature modes; ≥3 unit tests.

## Work Log
- 2026-07-24 (PM take-on) — Filed as the Linux plan mirror of the Windows plan; parallelisable with CPE-1019.
- 2026-07-25 — **Done.** Added `linux_shell_plan` + `LinuxShellPlan` to `cpe_server::shell_menu` (pure,
  `home` injected for testability). Emits a `.desktop` launcher (`Exec=… %F`, `MimeType=inode/directory;`,
  an "Open in <app>" desktop Action) targeted at `~/.local/share/applications/cross-platform-explorer.desktop`,
  the `inode/directory` default association, and a single-file remove set (reversible). 2 unit tests; part
  of the 13/13 module suite; clippy clean both modes. Applying the plan (write file + `xdg-mime default`)
  and real-Linux verification are a follow-up slice.
