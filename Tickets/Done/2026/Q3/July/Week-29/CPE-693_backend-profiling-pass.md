---
id: CPE-693
title: Backend listing profiling pass
type: task
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-688
estimate: 1-2h
---

## Summary
Child of CPE-688. Profile the directory walk on a large folder to confirm/deny it's in budget; optimize
Rust (parallel metadata, fewer allocations in dir_entry_from) only if the profile warrants — the diagnosis
expects the cost is in the webview, not Rust.

## Acceptance Criteria
- [x] Walk time measured on a large folder; conclusion recorded (in/out of budget).
- [x] Any Rust optimization justified by the profile; clippy clean both modes if touched.

## Work Log

## Resolution
Added an `#[ignore]`d profiling test `stream_dir_entries_walk_profile` (run with
`cargo test walk_profile -- --ignored --nocapture`); it stays out of the normal suite/CI so it adds no
time. Measured on this machine: **5,000 entries walked in 5.5 ms (~903 entries/ms)** → ~11 ms for 10k,
~55 ms for 50k. **Conclusion: the backend directory walk is NOT the bottleneck** — it's well within budget,
confirming CPE-688's diagnosis that the multi-second folder-open cost is entirely in the frontend render
pipeline (coalescing landed in CPE-689; virtualization is CPE-690). No Rust optimization warranted; the
walk already reuses FindNextFile metadata with no extra syscall. clippy clean both feature modes.
Files: src-tauri/src/lib.rs.
