---
id: CPE-821
title: Extract the backup copy engine into cpe-server
type: refactor
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-20
closed: 2026-07-21
epic: CPE-810
estimate: 2-3h
---

## Summary
Follow-up to CPE-815. Move the backup copy engine (CPE-797, epic CPE-736) — `safe_join`,
`copy_one_verified`, the `apply_backup_plan` walker + collect/stream commands — into `cpe_server::backup`.
It's pure filesystem domain logic that already leans on the extracted `OpResult` (`cpe_server::model`) and
`sha256_file` (`cpe_server::fsutil`), so it belongs in the Server. Apply the established **streaming split**:
the pure plan executor (with a per-file `flush` callback) lives in the crate; the `apply_backup_plan_stream`
command keeps its `ipc::Channel` in the Tauri adapter and feeds the walker.

## Acceptance Criteria
- [x] `safe_join` / `copy_one_verified` / the plan executor move to `cpe_server::backup`; `OpResult` +
      `sha256_file` reused from the crate.
- [x] The collect `apply_backup_plan` command dispatches; the streaming command keeps its `Channel` in the
      app and feeds the extracted executor via a callback.
- [x] The safe-join escape tests + copy/update/mirror tests move with the code and pass headless.
- [x] clippy clean both modes; behaviour-preserving.

## Work Log
2026-07-21 — Picked up (follow-up to CPE-815). Moved the backup copy engine (`safe_join`,
`copy_one_verified`, the `apply_backup_plan_walk` executor + a collect helper) into `cpe_server::backup`,
reusing `cpe_server::model::OpResult` + `cpe_server::fsutil::sha256_file`. Streaming split preserved: the
`apply_backup_plan_stream` command keeps its `ipc::Channel` + 16-item batching in the app and feeds the
extracted walker; `apply_backup_plan` dispatches to the collect helper. Moved the 3 tests (copy/verify,
mirror-delete, escape-rejection). Retired the last app user of `sha256_file`, so dropped it from the
app's fsutil re-export. Verified: `cpe-server` **100 tests** green; app `cargo test` **74 passed / 0
failed**; clippy `-D warnings` clean on both modes.

## Resolution
Extracted the backup copy engine (CPE-797) into **`cpe_server::backup`**: `safe_join` (path-escape guard),
`copy_one_verified` (copy + optional sha256 verify), `apply_backup_plan_walk` (the shared plan executor
with a `flush(OpResult)` callback), and an `apply_backup_plan` collect helper — all reusing the
already-extracted `OpResult` + `sha256_file`. The Tauri `apply_backup_plan` command dispatches; the
streaming `apply_backup_plan_stream` command keeps its `ipc::Channel` in the adapter and drives the same
walker. Tests moved; the app's now-unused `sha256_file` re-export was dropped. Files:
`crates/server/src/backup.rs` (new) + `pub mod backup;`; `src-tauri/src/lib.rs` (dispatchers).
