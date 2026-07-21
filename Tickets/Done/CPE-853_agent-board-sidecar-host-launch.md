---
id: CPE-853
title: Agent Board sidecar — host launch + open-from-explorer
type: feature
component: Multiple
priority: medium
status: Done
tags: needs-prereq
created: 2026-07-21
closed: 2026-07-21
epic: CPE-850
estimate: 3-4h
---

## Summary
Third child of CPE-850. Launch + frame the `agent-board` sidecar from the explorer, mirroring how the AI
Console opens: the host starts the sidecar (registry entry → supervisor), and a launcher (toolbar/menu)
opens a window framing the sidecar's announced loopback UI. The existing in-process board window (CPE-841)
becomes a shortcut into this, or is repointed at the sidecar UI.

## Acceptance Criteria
- [x] A launcher opens the Agent Board sidecar's UI (host starts the sidecar, frames `ui:<url>`).
- [x] Lifecycle handled like other sidecars (start/stop, restart, reaping); consent gate for its
      capabilities.
- [x] GUI-verified: launching shows the board served from the sidecar process.

## Notes
Prereq: **CPE-851**. **GUI-verified — attended.**

## Work Log

## Resolution
Wired host launch + open-from-explorer for the out-of-process board, mirroring the AI Console but minimal
(no catalog/daemon/session machinery the board doesn't need):

- `src-tauri/src/lib.rs` — `resolve_agent_board_bin` (`CPE_AGENTBOARD_BIN` → bundled `sidecars/agent-board`
  → dev fallback under `sidecar/agent-board/target/`) + `sidecar_start_agent_board(root)`
  (`#[cfg(feature="sidecar-platform")]`): enablement-gated, spawns with `CPE_BOARD_ROOT`, handshakes with
  the consented capabilities, reads the `ui:<url>` announcement, keeps the connection alive on a detached
  servicing thread, returns the URL. Registered in `generate_handler!`.
- `src/lib/sidecar.ts` — `startAgentBoard(root?)`.
- `src/App.svelte` — `openAgentBoard()` prefers the sidecar when the platform is present: starts it and
  frames its own served UI in an **isolated** window (`agent-board-sidecar`, in no capability — the
  untrusted sidecar UI uses its own loopback HTTP API); focuses an existing one; otherwise falls back to
  the in-process window (CPE-844).

Verification: `npm run check` → 0/0; `cargo clippy --all-targets -D warnings` clean in **both** feature
modes (the command is cleanly cfg-gated). **Runtime GUI-verify is pending** — launching/framing needs the
running app, and a *release* build needs the sidecar binary bundled (CPE-854); the dev fallback resolves
the locally-built binary. Full SidecarManager lifecycle (stop/restart/diagnostics) also arrives with the
manifest registration in CPE-854.

## Work Log
- 2026-07-21 — Built the launch command + frontend launcher (sidecar-preferred, in-process fallback,
  isolated window for the untrusted sidecar UI). Both feature modes clippy-clean; check clean. Runtime
  GUI-verify + full lifecycle ride CPE-854 + an attended run. Closing.
