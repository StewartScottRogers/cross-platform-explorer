---
id: CPE-730
title: "EPIC: Multi-agent conflict radar"
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
With Swarms and multiple sessions, detect live overlap between agents — the same file modified by two
sessions, one deleting what another is editing, competing renames — and raise a conflict banner + a per-file
"who else is here" indicator, colouring the heat-map by which agent owns each region.

## Why
Multiple agents on one tree can silently clobber each other. Surfacing contention live is exactly the kind
of visibility Agent Watch exists for, and it becomes essential as Swarms scale.

## Rough scope (areas, not child tickets)
- Cross-session activity attribution (activity isn't tagged by session today).
- An overlap/contention detector (same file, delete-vs-edit, competing renames).
- Conflict UI: banner + per-file "who else is here" indicator.
- Heat-map colouring by owning agent + a contention view.

## Open questions (resolve at activation)
- Getting reliable per-session attribution onto each activity event.
- Defining "conflict" precisely (temporal window, action pairs).
- How this interacts with Swarms coordination that may already partition work.

## Definition of Done
- Overlapping edits/deletes/renames across sessions are detected and flagged live.
- Each contended file shows which agents are involved; the heat-map colours by owner.
- No cost when only one session is running.

## Work Log
2026-07-22 (nightshift) — **Activated.** Open questions resolved (best-guess): "conflict" v1 = **edit-edit**
(2+ agents edit one file) + **edit-delete** (one deletes what another edits), ignoring same-agent
self-overlap; competing renames deferred (need source→target pairs); temporal windowing is a UI concern
layered later. First slice shipped: **CPE-914** — `conflict::detect_conflicts`. Remaining: reliable
per-session activity attribution feed + the radar UI (banner, "who else is here", owner-coloured heat-map).
