---
id: CPE-326
title: "Fix: Windows agent launch/detect fails on npm/script shims (error 193)"
type: Bug
status: Done
closed: 2026-07-13
priority: High
component: Backend
estimate: 45m
created: 2026-07-13
---

## Summary

Launching an agent on Windows fails with `%1 is not a valid Win32 application (os error
193)`: the CLI resolves to its **bare npm shim** (e.g. `…\npm\claude`, a shell script),
which `CreateProcess` can't execute. npm/pip installers create `claude.cmd` / `claude.ps1`,
not a bare `claude.exe`; the extensionless file is a Unix-style script. Same cause makes
`detect` report installed CLIs as **"not installed"** (the `<cli> --version` probe fails
identically).

## Fix

On Windows, run agent commands **through the shell** (`cmd /c <program> <args…>`) so the
shell applies PATHEXT and resolves the right shim. CLI-agnostic — works for any
npm/pip/etc. tool without per-manifest Windows entries. Applied at both spawn points:

- `ai-console::pty::PtySession::spawn` (the interactive launch),
- `ai-console::lifecycle::RealRunner` (detect/install/update/uninstall).

Non-Windows is unchanged (bare command). A `cli_command` helper does the wrap.

## Acceptance

- On Windows, an installed npm CLI (e.g. `claude`) detects as installed and launches into
  a working PTY session.
- Non-Windows behavior unchanged; tests green.

## Notes
Known follow-up: args containing cmd metacharacters (`&`, `%`, `|`) would be reinterpreted
by `cmd`; agent flags/models don't hit this in practice. Revisit if an agent needs it.

## Work Log
2026-07-13 — Added `cli_command` helper (cmd /c wrap on Windows, identity elsewhere);
applied at pty::PtySession::spawn and lifecycle::RealRunner. Verified on the target machine:
`cmd /c claude --version` -> "2.1.202 (Claude Code)" exit 0, where the bare npm shim fails
CreateProcess with 193. ai-console 69 tests + clippy green. Done.
