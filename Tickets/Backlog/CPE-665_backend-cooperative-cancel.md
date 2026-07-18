---
id: CPE-665
title: Cooperative cancellation for superseded dir streams
type: feature
component: Backend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-662
estimate: 1-2h
---

## Summary
Child of CPE-662. Today a superseded stream (user navigated away mid-load) is only dropped frontend-side;
the backend keeps walking a huge folder to completion invisibly. Add cooperative cancellation: a
generation/`AtomicBool` registry (or a cancel command keyed by a stream id) the walker checks between
batches so a superseded `list_dir_stream` stops promptly. Prereq: CPE-663/664.

## Acceptance Criteria
- [ ] A superseded stream stops walking within a batch or two rather than finishing the whole directory.
- [ ] No leaked registry entries (cleaned up on completion and cancellation); thread-safe.
- [ ] cargo-tested cancellation path; clippy clean both feature modes.

## Work Log
