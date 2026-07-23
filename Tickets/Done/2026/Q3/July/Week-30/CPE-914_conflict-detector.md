---
id: CPE-914
title: Multi-agent conflict detector (edit-edit / edit-delete)
type: feature
component: Sidecar
priority: medium
tags: ready
epic: CPE-730
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
First headless slice of the multi-agent conflict radar (CPE-730). `ai-console::conflict::detect_conflicts`
takes each agent/session's touched paths (`AgentActivity { agent, edited, deleted }`) and finds contention:
a path edited by **2+ agents** (`EditEdit`), or edited by one and **deleted by another** (`EditDelete`,
which takes precedence as the destructive case). Same-agent edit+delete is intentional, not a conflict.
Pure set logic; results sorted by path for a stable radar order.

## Acceptance Criteria
- [x] Detects edit-edit + edit-delete across agents; same-agent edit+delete is not a conflict.
- [x] Edit-delete takes precedence when both apply; involved agents listed (sorted).
- [x] 5 unit tests; clippy `-D warnings` clean.

## Work Log
- 2026-07-22 — Activated CPE-730 with the contention detector. Competing renames (need source→target
  pairs) + the live per-session attribution feed + the radar UI (banner / "who else is here" / owner
  heat-map) are the remaining children.
