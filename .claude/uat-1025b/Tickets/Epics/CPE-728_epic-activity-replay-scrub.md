---
id: CPE-728
title: "EPIC: Activity replay & scrub — a time machine for agent activity"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Turn the durable timeline into a time machine: a transport bar to scrub the session's filesystem history
backward and forward, replaying create/modify/move/delete/read in sequence while the folder view
reconstructs what it looked like at any moment.

## Why
The timeline answers "what happened"; replay answers "how did the agent get here?" after the fact — a
powerful way to understand and review an agent's work, not just watch it live.

## Rough scope (areas, not child tickets)
- Persist a full ordered event log (beyond the ~300-cap transient timeline).
- A virtual-time playback engine projecting folder state at time T.
- A scrubber/transport UI (play/pause/step/speed) driving the folder view.
- Jump-to-moment from any timeline entry.

## Open questions (resolve at activation)
- Event-log persistence format and size (shared with audit-log [[CPE-733]]?).
- Reconstructing folder state efficiently (snapshots vs. event replay).
- How replay coexists with a live session still producing events.

## Definition of Done
- A transport bar scrubs the session's history and the folder view reflects state at any point.
- Create/modify/move/delete/read replay in order with variable speed.
- The full ordered event log persists beyond the transient timeline cap.

## Work Log
2026-07-22 (nightshift) — **Activated.** First slice: **CPE-916** — `activity_timeline::bucketize` +
`summarize` over the existing `audit_journal::AuditEvent` stream (the scrub view's compute core). Remaining:
the timeline/minimap UI, playback scrubber, and state-at-cursor reconstruction.
