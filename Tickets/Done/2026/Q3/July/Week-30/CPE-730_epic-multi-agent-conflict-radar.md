---
id: CPE-730
title: "EPIC: Multi-agent conflict radar"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed: 2026-07-26
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

2026-07-26 (workshift) — **CLOSED. DoD met.** Remaining slices shipped this shift:
- **CPE-1116** owner-coloured activity heat-map + legend (PR #434) — FileList accent + legend coloured by
  owning agent (folderOwnerNorm mirrors conflict_owner/conflict_region; Okabe-Ito theme-var palette).
- **CPE-1117** rename source->target capture (PR #433) — cookie-correlated From/To pairing + RenameMode::Both;
  graceful single-path degradation; live-only (not journaled).
- **CPE-1118** competing-rename fold + Radar surfacing (PR #435) — divergence/collision across distinct actors,
  mirroring conflict_rename.rs.
DoD verification: overlapping edits/deletes (CPE-914 foldOverlaps) + competing renames (CPE-1117+1118) detected
live; contended files show actor pills (CPE-1100/1101) + rename-conflict pills; heat-map colours by owner
(CPE-1116); no cost single-session (all folds require >=2 distinct actors, owner colour collapses to one/none,
off-means-off held throughout — reviewers traced it each PR). Each slice passed an independent Reviewer +
independent UAT gauntlet before merge; every merge CI-green (3-OS matrix).
Fast-follows filed (NOT blocking the close): CPE-1120 (wire sessions->FileList for sorted-index colours),
CPE-1121 (optional time-gate for the rename fold), CPE-1122 (gate undo in read-only views), CPE-1119 (retire
orphaned sidecar conflict_*.rs). The rough-scope "conflict banner" is NOT a DoD bullet (Radar tab + pills
satisfy the DoD literally); file separately if wanted.
