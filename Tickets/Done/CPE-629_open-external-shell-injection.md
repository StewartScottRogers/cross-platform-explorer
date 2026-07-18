---
id: CPE-629
title: "Security: open_external could pass injection chars to cmd /C start"
type: Bug
component: Backend
priority: medium
status: Done
tags: ready
estimate: 30m
created: 2026-07-18
closed: 2026-07-18
---

## Summary
`open_external` (a frontend-reachable `#[tauri::command]`) opens a path on Windows via
`cmd /C start "" <path>`. `cmd.exe` re-parses its arguments, so a `"` in the path could break out of the
quoting and let following `&`/`|` inject a command. Defense-in-depth hardening: refuse paths containing
`"` (a reserved, impossible character in real Windows paths) or control characters before they reach the
shell — this closes the surface with zero functional loss (no legitimate path or URL contains them).

## Acceptance Criteria
- [x] `open_external` returns an error for a path containing `"` or any control character, before spawning.
- [x] Legitimate paths/URLs are unaffected (they can't contain those characters).
- [x] Unit test covers the rejection cases; clippy clean.

## Resolution
Added a guard at the top of `open_external`. Kept the existing open mechanism (the doc notes it was
chosen over the opener plugin for launch reliability), so there's no regression risk. A fuller move to
`ShellExecute`/the opener plugin Rust API remains a possible future hardening but needs GUI verification.

## Work Log
2026-07-18 (dayshift) — Found while auditing shell-invoking commands after the CPE-628 zip-slip fix.
run_as_admin was already safe (PowerShell single-quote escaping); open_external's cmd path was not.
