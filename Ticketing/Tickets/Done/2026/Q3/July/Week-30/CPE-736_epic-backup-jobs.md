---
id: CPE-736
title: "EPIC: Backup jobs"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed: 2026-07-21
---

## Goal
Named, one-click or scheduled backup jobs (source set -> destination, incremental, verified) with progress,
history, and restore — targeting plain folders to external drives or network shares.

## Why
Distinct from the git-based auto-mirror: this is ordinary-user backup to a USB drive or NAS. Reliable,
verified, incremental backup is a core trust feature for a daily-driver file manager.

## Rough scope (areas, not child tickets)
- Job definitions in settings (source set, destination, schedule, options).
- An incremental copy engine with checksum verification (reuse checksum + transfer primitives).
- A scheduler (interval / on-connect of a target drive).
- A job dashboard: last-run status, history, and one-click restore.

## Open questions (resolve at activation)
- Relationship to auto-mirror/sync-policy (autoMirror.ts) — extend or separate engine?
- Scheduling while the app is closed (service?) vs. only when open.
- Versioned backups vs. mirror; retention.

## Definition of Done
- Users define backup jobs (source->destination, incremental, verified) and run them one-click or scheduled.
- Jobs show progress + history; a completed backup can be restored.
- Verification catches copy errors; opt-in with no background cost when no job is scheduled.

## Work Log
2026-07-20 (autonomous) — Activated. Open questions resolved: **separate engine** from autoMirror (this is
plain folder→external/NAS, not git); **incremental** copy+update with **optional mirror-delete**; runs
while the app is open v1 (scheduler-while-closed deferred); versioned/retention deferred. The planner
**reuses CPE-777 folder-tree diff** to compute what a run would transfer. Pure planner first.

## Child tickets
1. **CPE-796** — Pure backup model + planner (`src/lib/backup.ts`): `planBackup(source, dest, mirror)` via
   `diffTrees` (CPE-777) → files to copy/update/delete + unchanged count; `BackupJob` CRUD + parse/serialize.
   Unit-tested. **Foundation, headless.**
2. **CPE-797** — Incremental copy engine + checksum verification + drive-connect scheduler (reuse transfer +
   checksum primitives). **Backend.** *(prereq: 796)*
3. **CPE-798** — Job dashboard: last-run status, history, one-click restore. **GUI.** *(prereq: 796, 797)*

## Resolution (closed 2026-07-21)
All child tickets are **Done** — the epic's Definition of Done is delivered by the backup-jobs engine (CPE-796/797/798). Closed as part of the
epic-queue tidy-up: every planned child shipped, no remaining scope. Feature verification lives in each
child's Resolution.
