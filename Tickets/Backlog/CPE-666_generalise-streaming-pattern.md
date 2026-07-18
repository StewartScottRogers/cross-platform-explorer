---
id: CPE-666
title: Generalise + document the streaming-liveness pattern
type: task
component: Multiple
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-662
estimate: 2-3h
---

## Summary
Final child of CPE-662 (its DoD "applied to at least one other producer + documented"). Apply the
channel-streaming shape to one more large/slow producer beyond `list_dir` (e.g. `read_archive_entries` or
aligning the search results path), and write up the house style — when to stream vs. return a vec, the
`ipc::Channel` batch convention, and the generation-token supersede — as a design doc under
`docs/design/`. Prereq: CPE-664.

## Acceptance Criteria
- [ ] A second bulk producer streams via `ipc::Channel` with progressive frontend render.
- [ ] `docs/design/STREAMING.md` (or similar) documents the pattern; in-app docs updated if user-facing.
- [ ] `npm run check` clean; suite green.

## Work Log
