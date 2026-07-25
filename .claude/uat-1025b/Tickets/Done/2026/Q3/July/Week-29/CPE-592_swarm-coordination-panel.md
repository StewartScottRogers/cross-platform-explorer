---
id: CPE-592
title: "AI Console: live Swarm Coordination panel (mailbox + memory feed)"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [ready]
epic: CPE-528
created: 2026-07-17
closed: 2026-07-17
---

## Summary
A swarm coordinated correctly but you could only see it via the files / a script. Add a **live panel in
the AI Console** that shows the shared **mailbox** posts and **memory** notes updating in real time, so
agents' coordination is visible on screen (the point of Agent Watch).

## Changes
- `console.rs` — `GET /api/swarm/activity?mission=cpe-swarm-<id>` (`handle_swarm_activity`): returns the
  mission's `mailbox.jsonl` posts + parsed `memory/` notes. Reads `req.query("mission")`; the id must be a
  bare `cpe-swarm-<digits>` resolved **under the OS temp dir only** (no separators/traversal).
- `launcher.html` — a `#swarm-panel` (Mailbox | Shared memory columns) that opens when a swarm starts
  (using the `mission` returned by `/api/swarm/run`) and polls the activity endpoint every 2s; a Hide
  button stops it. The kind badge follows the pill rule (nowrap, no shrink).

## Acceptance Criteria
- [x] `GET /api/swarm/activity` returns mailbox + memory for a mission; rejects non-`cpe-swarm` ids /
      traversal (400).
- [x] Running a swarm opens the panel and renders posts + notes live; Hide closes + stops polling.
- [x] Tests: backend (feed + security) + jsdom (panel opens, renders mailbox/memory). Sidecar 290 +
      frontend suites + clippy + check green.

## Notes
Completes the observability goal of epic [[CPE-528]] — you can now watch the swarm coordinate, not just
inspect files afterward (though `verify-swarm.ps1` still works for a CLI check).
