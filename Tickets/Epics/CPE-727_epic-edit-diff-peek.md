---
id: CPE-727
title: "EPIC: Edit diff peek — see what each agent write changed"
type: Task
status: Proposed
priority: High
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Capture a before/after snapshot per mutation and render an inline, scrubbing-friendly diff: hover a touched
row or timeline entry to see the exact hunks the agent wrote, click to open a full side-by-side.

## Why
Every `modified`/`created` event annotates a row but shows nothing about *what* changed. The single most
useful thing about a write is its content — this is squarely on Agent Watch's visibility tiebreaker.

## Rough scope (areas, not child tickets)
- Content snapshotting on watch events (bounded, size-capped) to a diff store.
- Hunk computation reusing the existing diff renderer.
- Inline diff peek on touched rows + timeline entries; full side-by-side on click.
- Eviction/retention policy for snapshots.

## Open questions (resolve at activation)
- Snapshot size caps and which files to snapshot (skip binaries/huge files?).
- Storage location and retention of before/after content.
- Interaction with checkpoint/rollback ([[CPE-732]]) which also needs snapshots.

## Definition of Done
- Hovering a touched row/timeline entry shows the exact hunks written; click opens side-by-side.
- Snapshots are bounded and evicted per policy; large/binary files degrade gracefully.
- With Agent Watch off, no snapshotting occurs.
