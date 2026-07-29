---
id: CPE-807
title: Fix flaky audit_journal test tmp() dir collision (macOS CI red)
type: bug
status: Done
priority: high
component: Backend
tags: ready
created: 2026-07-20
closed: 2026-07-20
epic: CPE-733
estimate: 15m
---

## Summary
The `audit_journal` test helper `tmp()` (added in CPE-800) built its scratch-dir name from
`process::id()` + `SystemTime::now()` nanos. cargo runs tests in parallel and the process id is shared, so
on a platform with a coarse clock (macOS CI) two tests minted the **same** directory and clobbered each
other's `s1.jsonl` — making `detail_round_trips_and_missing_session_is_empty` fail intermittently. It
passed on the Windows PR runs by timing luck, so CPE-800 merged green but the **main-branch** post-merge
runs for #40 (CPE-800) and #41 (CPE-797, which carries the same tests) went red.

## Acceptance Criteria
- [x] `tmp()` yields a unique dir per call regardless of clock resolution or parallel scheduling.
- [x] `cargo test audit_journal` green; clippy `--all-targets -D warnings` clean (both feature modes).
- [x] main-branch CI green again.

## Resolution
Replaced the time-based name with a process-wide `AtomicU64` counter (`SEQ.fetch_add`), so every `tmp()`
call is unique by construction — no reliance on clock granularity. Test-only change in
`src-tauri/src/audit_journal.rs`; the production journal code is untouched. Verified locally (6/6
audit_journal tests pass, clippy clean in both feature modes). The sibling backup-engine tests (CPE-797)
use the pre-existing `scratch(tag)` helper keyed by a distinct per-test tag, so they were never affected.

Root cause noted for future test helpers: never derive a "unique" temp path from `process::id()` + a
timestamp alone under parallel tests — use an atomic counter (or the distinct-tag `scratch` helper).
