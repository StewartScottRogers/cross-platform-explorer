---
id: CPE-693
title: Backend listing profiling pass
type: task
component: Backend
priority: low
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-688
estimate: 1-2h
---

## Summary
Child of CPE-688. Profile the directory walk on a large folder to confirm/deny it's in budget; optimize
Rust (parallel metadata, fewer allocations in dir_entry_from) only if the profile warrants — the diagnosis
expects the cost is in the webview, not Rust.

## Acceptance Criteria
- [ ] Walk time measured on a large folder; conclusion recorded (in/out of budget).
- [ ] Any Rust optimization justified by the profile; clippy clean both modes if touched.

## Work Log
