---
id: CPE-797
title: Incremental backup copy engine + verification + scheduler
type: feature
status: Open
priority: medium
component: Multiple
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-736
estimate: 4h+
---

## Summary
Backend for epic CPE-736: execute a CPE-796 plan — incremental copy/update (+ optional mirror-delete) with
checksum verification, streamed progress, and a run on demand or when the target drive connects. Reuse the
transfer + sha256 primitives.

## Acceptance Criteria
- [ ] A job copies/updates changed files, optionally deletes extraneous (mirror), verifies by checksum.
- [ ] Streamed progress; opt-in; no background cost when no job is scheduled; errors surfaced per file.
- [ ] cargo/CI green.

## Notes
Prereq: CPE-796. Runs while the app is open (v1). Reuse transfer-manager + checksum backend.
