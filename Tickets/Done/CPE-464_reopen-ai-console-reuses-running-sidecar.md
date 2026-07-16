---
id: CPE-464
title: "Reopening the AI Console spawns a fresh sidecar, losing all sessions"
type: Defect
status: Done
priority: High
component: Backend
tags: [ready]
created: 2026-07-15
closed: 2026-07-15
epic: CPE-261
---

## Summary
Closing the AI Console window and reopening it loses all agent tabs — CPE-461's reattach finds
nothing. Root cause: reopening starts a **brand-new** sidecar instead of reusing the running one.

## Root cause
`sidecar_start_ai_console` unconditionally `spawn_process_with_env(...)`s a new sidecar and stores its
connection in `AiConsoleState.conn`, **dropping the previous `ConsoleConn`** — which reaps the old
sidecar process (and its live agent PTYs). So on reopen: a fresh sidecar with an empty `ConsoleState`
serves the new window → `/api/sessions` is empty → no tabs reattach, and the previously-running agents
are killed. (CPE-461's reattach was correct but had no live sidecar to reattach to.)

## Fix
Reuse the already-running sidecar: if `state.conn` is `Some` and we have its served URL, return that
URL instead of spawning a second sidecar. The reopened window then loads the SAME sidecar, whose
`ConsoleState.sessions` still hold the agents, and CPE-461's `reattachSessions()` restores every tab.
Store the served `ui:` URL in `AiConsoleState` when the sidecar starts; clear it on stop/disable.

## Acceptance Criteria
- [x] `sidecar_start_ai_console` returns the running sidecar's URL when one is live (no second spawn).
- [x] The served URL is stored on start and cleared on stop/disable.
- [x] Closing + reopening the AI Console reattaches the existing agent tabs (with CPE-461), and the
      agents are NOT killed by the reopen.
- [x] Compiles + clippy clean in both feature modes.

## Notes
This also makes the explorer left-pane "Agents" leaves consistent with the console (both reflect the
one live sidecar). Full verification is a GUI run: open console, launch 2 agents, close the window,
reopen → both tabs return.

## Resolution
`AiConsoleState` gains a `url: Mutex<Option<String>>`. `sidecar_start_ai_console` now, right after the enable check, **returns the running sidecar's stored URL when `conn` is live** instead of spawning a second sidecar (which dropped the old `ConsoleConn` → reaped the old process + its agents). The served `ui:` URL is stored on start and cleared on `sidecar_stop` / disable. So closing + reopening the AI Console window loads the SAME live sidecar, whose `ConsoleState.sessions` still hold the agents, and CPE-461's `reattachSessions()` restores every tab. clippy `-D warnings` clean in both feature modes. This is a host-runtime fix — the actual reattach (open console → launch 2 agents → close window → reopen → both tabs return) is GUI-verified.
