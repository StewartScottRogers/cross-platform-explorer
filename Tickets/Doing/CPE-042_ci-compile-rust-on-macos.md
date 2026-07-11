---
id: CPE-042
title: CI must compile the Rust backend on macOS too
type: Test
status: Open
priority: High
component: CI
estimate: 20m
created: 2026-07-11
closed:
---

## Summary

The backend CI matrix covers ubuntu and windows (CPE-028) — but the release workflow **also builds
macOS**. So macOS-specific compile failures are still invisible until a tag is pushed, which is the
exact class of bug CPE-028 existed to kill.

This became concrete while planning CPE-041 (undo): restoring from the Recycle Bin uses
`trash::os_limited`, which is **not implemented on macOS**. Without a macOS CI job I have no safe way
to know whether such code compiles there.

## Acceptance Criteria

- [ ] Backend CI matrix includes macos-latest
- [ ] cargo check / clippy / test run on macOS
- [ ] macOS-only compile failures are caught before a release tag

## Resolution
## Work Log
## Notes
Prerequisite for safely writing any platform-gated Rust.
