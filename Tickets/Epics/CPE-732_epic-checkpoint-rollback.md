---
id: CPE-732
title: "EPIC: Checkpoint & rollback of agent work"
type: Task
status: In Progress
priority: High
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Let the user snapshot the watched tree at moments during a session and one-click revert the agent's
filesystem changes back to any checkpoint, or cherry-revert a single mutation from the timeline.

## Why
The safety net that makes watching an agent low-stakes: if it goes wrong, undo it. Complements diff-peek and
audit by making the history not just visible but reversible.

## Rough scope (areas, not child tickets)
- Efficient content-addressed snapshots of the watched subtree (bounded, dedup).
- A revert engine that respects the "skip unreadable entries" rule.
- Checkpoint markers on the timeline + a restore UI.
- Single-mutation cherry-revert from a timeline entry.

## Open questions (resolve at activation)
- Snapshot storage/size budget and dedup strategy for large trees.
- Shared snapshot store with diff-peek ([[CPE-727]]) and replay ([[CPE-728]]).
- Revert safety when files changed outside the agent since the checkpoint.

## Definition of Done
- Users can checkpoint the watched tree and one-click revert the agent's changes to any checkpoint.
- A single mutation can be cherry-reverted from the timeline.
- Snapshots are bounded/deduped; revert respects unreadable-entry handling.

## Work Log
2026-07-22 (nightshift) — **Activated.** First slice: **CPE-917** — `restore_plan::plan_restore` /
`revert_one` / `summarize_plan`: the pure diff that turns a checkpoint snapshot + current tree state into the
minimal Create/Overwrite/Delete revert ops (+ cherry-revert of one path). Remaining: content-addressed
snapshot capture/store (dedup, bounded), the revert engine, and the timeline checkpoint-marker + restore UI.
