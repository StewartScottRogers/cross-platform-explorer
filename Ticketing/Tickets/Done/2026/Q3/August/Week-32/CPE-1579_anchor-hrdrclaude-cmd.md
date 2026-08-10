---
id: CPE-1579
title: "hrdrClaudeNative.cmd: anchor to its own folder (cd /d %~dp0)"
type: Task
status: Done
priority: Low
component: Tooling
tags: [ready]
created: 2026-08-10
closed: 2026-08-10
---

## Why
User request (2026-08-10). `hrdrClaudeNative.cmd` computed its own folder into `SELF_DIR` (`%~dp0`) for
checkout detection but never changed the working directory to it, so the launched herdr server and any
relative operations inherited the caller's cwd instead of the script's location.

## Change
Added `cd /d "%~dp0"` immediately after `setlocal`, so the script runs anchored to its own folder no
matter where it's invoked from (matching RunClaude.cmd's `%~dp0` path-independence). One-line, no
behavior change beyond the deterministic cwd.

## Verify
Batch syntax valid (`cd /d "%~dp0"` is the standard anchor idiom); no test surface. Foreman-applied per
direct user request.
