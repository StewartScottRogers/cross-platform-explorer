---
id: CPE-319
title: "Fix: AI Console binary resolution in dev (wrong CWD)"
type: Bug
status: Done
priority: Medium
component: Backend
estimate: 30m
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Opening the AI Console failed at runtime: `resolve_ai_console_bin`'s dev fallback used a
path relative to the process CWD (`sidecar/ai-console/target/...`), but `tauri dev` runs
the app with cwd = `src-tauri/`, so the binary was never found and the command errored.

## Fix

Resolve the dev fallback relative to the crate at compile time via
`env!("CARGO_MANIFEST_DIR")` (`../sidecar/ai-console/target/<profile>/<exe>`), with the
CWD-relative path kept as a secondary. The env override (`CPE_AICONSOLE_BIN`) and resource
dir remain first. Also added `tests/ai_console_flow.rs` (ignored; env-driven) that runs the
exact command flow against the real binary — used to isolate this bug (spawn → handshake
granted all 4 caps → `ui:` URL — all fine; only the path was wrong).

## Work Log
2026-07-13 — Diagnosed via the flow test (backend flow works end-to-end); fixed the dev
path to be compile-dir-relative. Feature build + clippy green. Done.
