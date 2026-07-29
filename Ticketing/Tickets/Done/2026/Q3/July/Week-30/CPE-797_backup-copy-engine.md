---
id: CPE-797
title: Incremental backup copy engine + verification + scheduler
type: feature
status: Done
priority: medium
component: Multiple
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-736
estimate: 4h+
---

## Summary
Backend for epic CPE-736: execute a CPE-796 plan — incremental copy/update (+ optional mirror-delete) with
checksum verification, streamed progress, and a run on demand or when the target drive connects. Reuse the
transfer + sha256 primitives.

## Acceptance Criteria
- [x] A job copies/updates changed files, optionally deletes extraneous (mirror), verifies by checksum.
- [x] Streamed progress; opt-in; no background cost when no job is scheduled; errors surfaced per file.
      *(Per-file `OpResult` errors ✓; opt-in/no-background-cost ✓; **streamed progress** landed with the
      CPE-798 dashboard (`apply_backup_plan_stream` over an `ipc::Channel`); the **drive-connect scheduler**
      landed 2026-07-20 — polls only when a job opts into auto-run.)*
- [x] cargo/CI green.

## Notes
Prereq: CPE-796. Runs while the app is open (v1). Reuse transfer-manager + checksum backend.

## Work Log
- 2026-07-20 — Picked up. Grepped the backend first: existing `copy_dir_all` / `copy_entries` /
  `copy_tree_streamed` / `transfer_registry` / `sha256_file` primitives to reuse (per the ticket's "reuse"
  note). The frontend `planBackup` (src/lib/backup.ts, CPE-796) already emits flattened relative
  copy/update/delete lists, so the missing backend piece is a *plan executor*, not another walker.
- 2026-07-20 — Built the headless copy-engine core: `apply_backup_plan_impl` +
  `apply_backup_plan` command. Copies new/changed files (creating parent dirs), mirror-deletes extraneous
  ones, verifies each written file by sha256, and returns a per-file `OpResult` (never all-or-nothing). A
  `safe_join` guard rejects any plan-relative path that would escape the dest root (`..`, absolute, drive
  prefix), so a malformed plan can't widen the blast radius. Registered in `generate_handler!`.
- 2026-07-20 — 3 cargo tests (copy+update+verify keeps unrelated files; mirror-delete + per-file reporting
  of a missing entry; path-escape rejection writes nothing outside dest). Clippy clean in BOTH feature
  modes, `--all-targets -D warnings`. Added `Debug` to `OpResult`.
- 2026-07-20 — **Deferred.** The copy/verify/mirror core (AC1) + per-file error surfacing land now and are
  CI-green. Remaining: **streamed progress** over an `ipc::Channel` (per docs/design/STREAMING.md) and the
  **on-drive-connect scheduler** — both pair with the backup dashboard GUI (CPE-798), so they're better
  built attended alongside that view than blind here.
  - *deferred-on:* its own streaming/scheduler tail (an internal follow-up), best done with CPE-798.
  - *revisit-when:* picking up CPE-798 (backup dashboard) — wire `apply_backup_plan` to a streaming
    channel + a drive-arrival trigger there. No external gate; pickable anytime.

## Resolution (partial — core landed, tail deferred)
Landed the deterministic backend copy engine in `src-tauri/src/lib.rs`: `apply_backup_plan(source_root,
dest_root, copy, update, delete, verify)` executes a `planBackup` result — copy/overwrite the listed
relative files (creating parent dirs), mirror-delete the extraneous ones, sha256-verify each write — and
returns a per-file `OpResult`. `safe_join` prevents any plan path from escaping the dest root. Reuses the
existing `sha256_file`. Three tempdir cargo tests cover the happy path, mirror-delete + per-file error
reporting, and path-escape rejection. Streamed progress + the scheduler are deferred to CPE-798 (see Work
Log) so they're built with the dashboard that consumes them.

## Update — streamed progress + drive-connect scheduler landed + GUI-verified (2026-07-20)
Both deferred tails are done, completing all three ACs:
- **Streamed progress** (with the CPE-798 dashboard): `apply_backup_plan_stream` batches per-file
  `OpResult`s over an `ipc::Channel` (docs/design/STREAMING.md); the dashboard row shows live `n / total`.
- **Drive-connect scheduler** `src/lib/driveScheduler.ts`: pure `driveRoot` / `newlyConnected` /
  `jobsForConnect` / `anyAutoRun`, plus a poller that reads `list_drives` every 15s and runs an **auto-run**
  job when its destination drive transitions absent→present. Only the *connect* fires (drives present at
  startup are seeded; a drive that stays plugged in doesn't re-run), and the poller stays **off** unless a
  job opts in (`BackupJob.autoRun`) — honouring "no background cost when no job is scheduled". App wires
  `runBackupJobNow` (the same streamed apply the dashboard uses → history + notice) and reconciles the
  scheduler on job changes / at startup; the dashboard grows a per-job **"auto-run on connect"** toggle +
  an `auto` pill. 7 unit tests (`driveScheduler.test.ts`).

**GUI-verified end-to-end in the sidecar dev build (CDP):** live `list_drives` → `["C:\\","Z:\\"]`; simulating
a `C:\` connect transition, `jobsForConnect` returned **only the auto-run job** (the opted-out one was
skipped), and a drive **already** connected produced **no** fire; the selected job then ran through the real
`apply_backup_plan` → both files (incl. nested `sub/b.txt`) copied `ok` with checksum verify, landing in
dest. Frontend suite **884 green**; `npm run check` clean.

## Resolution
Backend copy engine (`apply_backup_plan` / `apply_backup_plan_stream` + `safe_join`) executes a `planBackup`
result with per-file `OpResult` + sha256 verify + streamed progress; the frontend `driveScheduler.ts` runs
an opted-in job automatically when its destination drive connects. All ACs met; CI green. CPE-797 → Done.
