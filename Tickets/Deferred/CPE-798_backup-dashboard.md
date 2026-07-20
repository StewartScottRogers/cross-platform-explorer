---
id: CPE-798
title: Backup job dashboard (status / history / restore)
type: feature
status: Deferred
priority: medium
component: Frontend
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-736
estimate: 3-4h
---

## Summary
The UI for epic CPE-736: define/run backup jobs, show last-run status + history, a dry-run preview (via the
CPE-796 planner), and one-click restore.

## Acceptance Criteria
- [~] Create/run jobs; dashboard shows progress, last-run status, and history; dry-run preview lists changes.
      *(create/run/dry-run + **last-run status** + change-list counts done & verified; **live progress** (streamed) and **multi-run history** need the CPE-797 streaming tail — follow-up.)*
- [x] One-click restore from a completed backup; menus follow MENUS.md.
- [~] check + suite green; GUI-verified.
      *(`npm run check` clean + GUI-verified now; live-progress streaming is the deferred part.)*

## Notes
Prereq: CPE-796, CPE-797. Attended GUI.

## Resolution (core dashboard shipped + verified; live progress/history deferred)
Built `src/lib/components/BackupDashboard.svelte` over the tested planner + my copy-engine/scan backend:
define source→dest jobs (name/source/dest/mirror, persisted via `settings.cpe.backupJobs`), **dry-run** a
plan (two `scan_tree` scans → `planBackup`, CPE-796) showing copy/update/delete/unchanged counts, **run** it
(`apply_backup_plan`, CPE-797, with checksum verify) with a per-job last-run status (ok/failed + time), and
**one-click restore** (the reverse copy — backup → source). Opened from the command palette ("Backup
jobs…", all 12 locales).

**GUI-verified in the running dev app (CDP):** created a job over a controlled pair (source: 3 files incl.
`sub/c.txt`; empty dest) → **dry-run = "3 copy · 0 update · 0 delete · 0 unchanged"** → **Run → "backup: 3
ok"** and all 3 files (incl. the nested one) appeared in dest on disk → deleted a source file → **Restore →
"restore: 1 ok"** and the file returned to source from the backup. Test job + folders cleaned up.
`npm run check` clean; backup/settings/i18n suites green (54).

Deferred tail (AC1 remainder): **live streamed progress** during a run (a big job currently returns one
`OpResult[]` at the end — the streamed variant is the CPE-797 tail) and **multi-run history** (only the last
run is shown). Build both alongside the CPE-797 streaming/scheduler tail. No external gate.

## Update — live streamed progress landed (2026-07-20, hard-bucket)
Added `apply_backup_plan_stream` (backend): refactored the plan executor into a shared `apply_backup_plan_walk`
that invokes a per-result callback — the collect command and the new streaming command both drive it (one
walker, per docs/design/STREAMING.md). The streaming command sends each file's `OpResult` over an
`ipc::Channel` in batches of 16 as it completes. `BackupDashboard` runs via `rawInvoke` + a `Channel` and
shows **live `running… N / M`** progress, then the final status.

**GUI-verified (CDP):** a 30-file backup showed a mid-run sample of **"running… 16 / 30"** (the first batch
flushed before completion) → final **"backup: 30 ok"** with 30 files on disk. `npm run check` clean; the
existing apply_backup_plan cargo tests still pass through the refactored walker; clippy clean both modes.

Remaining (small): **multi-run history** (the dashboard shows only the last run) — a per-job ring of recent
runs is the last follow-up. Also advances CPE-797's streaming tail (the scheduler / on-drive-connect trigger
is the other half).
