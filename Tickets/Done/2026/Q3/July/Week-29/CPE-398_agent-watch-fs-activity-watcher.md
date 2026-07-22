---
id: CPE-398
title: Agent Watch — filesystem activity watcher (notify), gated + idle-when-off
type: feature
priority: high
estimate: L
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [big-design, agent-watch, backend]
epic: AGENT-WATCH.md
depends-on: CPE-396
---

## Goal
Observe the agent's filesystem mutations live so the Agent Watch view (CPE-399) can annotate
them. A `notify`-crate watcher on the active session's Project Folder, streaming
create/modify/move/delete events to the frontend via Tauri events.

## Design notes
- Agent-agnostic (watches the folder, not the agent) — matches AGENT-WATCH.md's "additive
  layer over the existing filesystem commands."
- **Reads are out of scope here** — a Windows FS watcher can't see reads; reads would need the
  agent's own tool-output stream (agent-specific), tracked separately as a later enhancement.
- Debounce/coalesce bursts; cap event volume so a big refactor can't flood the UI.

## Off means off (hard constraint)
Watcher is created only while a session is active and being watched; on session end / mode
off it is torn down. No watcher, no polling, no thread when idle. Feature-gated.

## Acceptance
- [x] Mutations under the watched folder arrive in the frontend as typed events
- [x] Watcher starts on watch, stops on unwatch/exit — verified no residual watcher when off
- [x] Debounced; bounded under heavy churn
