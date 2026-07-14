---
id: CPE-331
title: "Robust uninstall: native binary + running-session guard"
type: Feature
status: Done
closed: 2026-07-13
priority: Low
component: Backend
created: 2026-07-13
---

## Summary

Reference uninstallers (a) refuse if a live agent session holds a file lock (e.g. Claude
sets CLAUDECODE=1) to avoid a half-removed state, and (b) remove BOTH the npm global
package AND the native-installer binary (`~/.local/bin/claude.exe`), leaving user config
intact. Ours runs only `npm rm -g`, so a native-installed CLI or a locked binary is left
behind. Steal: detect a running session and bail with guidance; remove the native binary
too; report what was/wasn't removed.

## Acceptance
- Uninstall removes both install methods and refuses cleanly (with guidance) when the
  agent is running.

## Work Log
2026-07-13 (Dayshift) — Hardened `lifecycle::uninstall` on branch `CPE-331-robust-uninstall`.

- **Running-session guard:** `uninstall(agent, runner, running)` now refuses with a clear
  message when `running` is true, so the CLI can't be left half-removed. Batch uninstall via
  `aggregate.rs` passes `false`; a session-aware route can pass the real state.
- **Remove BOTH artifacts:** after the npm/pip uninstall recipe, it now also removes a
  native-installer binary left behind at `~/.local/bin/<cli>[.exe]` (the reported defect —
  we previously ran only `npm rm -g`) and reports what was/wasn't removed. Never touches
  shared prerequisites or user config.
- Factored testable helpers `native_bin_dir` / `native_binary_candidates` (leaf-name only,
  `.exe`-first on Windows) / `remove_native_binary`; `home_root()` from USERPROFILE/HOME.

Tests: +4 (refuses-while-running; candidate paths on win/unix; leaf-name; real remove via a
tempdir home + no-op second call); updated the existing uninstall test for the new arg.
`cargo test` 87 lib + 7 integration pass; `cargo clippy --all-targets` clean.

Assumptions (Dayshift, user away): the CLI name for the native-binary path is the agent's
`run` command program (e.g. `claude`), and native installers use the reference's
`~/.local/bin` location. The guard is wired at the function/aggregate layer; a dedicated
user-facing uninstall route/UI doesn't exist yet (uninstall is reached via aggregate
actions), so end-to-end refusal-on-running will be exercised once that route is added.
Real filesystem removal of a live native install still warrants a manual check.
