---
id: CPE-735
title: "EPIC: Local snapshots / time-machine-lite"
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
Point-in-time, space-efficient local versioning of chosen folders (hardlink/CoW snapshots where the
filesystem supports it, copy fallback elsewhere), with a timeline browser to view, diff, and restore any
file or whole folder to an earlier state.

## Why
Gives casual "undo across sessions" and accidental-overwrite recovery without a cloud — a safety net for
ordinary users that the current session-scoped undo can't provide.

## Rough scope (areas, not child tickets)
- A snapshot scheduler (manual + interval) for chosen folders.
- A dedup/CoW store (hardlink/reflink where supported, copy fallback).
- Retention/prune policy with a size budget.
- A restore UI: browse snapshots, diff, restore a file or whole folder.

## Open questions (resolve at activation)
- CoW/reflink support detection per filesystem; fallback cost.
- Distinction and overlap with backup jobs ([[CPE-736]]) and agent checkpoints ([[CPE-732]]).
- Storage budget and prune defaults.

## Definition of Done
- Chosen folders can be snapshotted (manually and on a schedule) space-efficiently.
- Users can browse the timeline, diff, and restore a file or whole folder to an earlier state.
- Retention prunes to budget; the feature is opt-in with no cost when unused.

## Work Log
2026-07-23 (dayshift) — **Activated.** First slice: **CPE-944** — `snapshot_retention::thin`: the
grandfather-father-son (hourly/daily/weekly/monthly) keep-vs-prune policy. Remaining: the snapshot capture
engine (content-addressed, deduped) and the timeline/restore UI.
