---
id: CPE-736
title: "EPIC: Backup jobs"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
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
