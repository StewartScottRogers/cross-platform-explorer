---
id: CPE-325
title: "Fix: sidecar spawns flash console windows and hang the UI on Windows"
type: Bug
status: Done
closed: 2026-07-13
priority: High
component: Backend
estimate: 1h
created: 2026-07-13
---

## Summary

Opening the AI Console on Windows flashes several console windows that never fully render
and vanish, while the app appears hung for a moment before recovering. Cause: subprocesses
are spawned without `CREATE_NO_WINDOW`, so Windows pops a console window for each. The
worst offender is the launcher catalog, which runs an install-check (`<cli> --version`)
for **all 12 agents sequentially** — 12 flashing windows + a multi-second stall — every
time the launcher loads. The sidecar spawn itself and MCP servers flash too.

## Fix

1. Apply `CREATE_NO_WINDOW` (0x08000000) on Windows to every subprocess spawn:
   - `sidecar-host::supervisor::spawn_process` (the sidecar itself),
   - `ai-console::lifecycle::RealRunner` (detect/install/update/uninstall),
   - `ai-console::mcp::McpProcess::spawn` (MCP servers).
   (PTY launches go through ConPTY, which is already windowless.)
2. Parallelize the catalog install-detect with scoped threads so 12 probes run at once
   instead of serially — removes the stall.

## Acceptance

- No console windows flash when opening the console or loading the launcher on Windows.
- Catalog loads without a visible hang.
- Non-Windows unaffected; all sidecar tests still green.

## Work Log
2026-07-13 — Applied CREATE_NO_WINDOW (0x08000000) on Windows to spawn_process (host),
RealRunner (lifecycle), and McpProcess::spawn via a `hide_console` helper; parallelized the
catalog install-detect with `std::thread::scope` (12 probes concurrent, not serial). Also
de-flaked the live-server http test (retry the loopback exchange). host 77 + ai-console 69
tests green, feature clippy clean. Done.
