---
id: CPE-042
title: CI must compile the Rust backend on macOS too
type: Test
status: Done
priority: High
component: CI
estimate: 20m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The backend CI matrix covers ubuntu and windows (CPE-028) — but the release workflow **also builds
macOS**. So macOS-specific compile failures are still invisible until a tag is pushed, which is the
exact class of bug CPE-028 existed to kill.

This became concrete while planning CPE-041 (undo): restoring from the Recycle Bin uses
`trash::os_limited`, which is **not implemented on macOS**. Without a macOS CI job I have no safe way
to know whether such code compiles there.

## Acceptance Criteria

- [x] Backend CI matrix includes macos-latest
- [x] cargo check / clippy / test run on macOS
- [x] macOS-only compile failures are caught before a release tag

## Resolution

Added `macos-latest` to the backend CI matrix. All three OSes we ship now run cargo check, clippy
`-D warnings`, and cargo test on every push.

This closed a real hole: `release.yml` builds a macOS bundle, but nothing compiled macOS until a tag
was pushed. It became urgent while planning CPE-041, where the obvious implementation depends on a
crate API that doesn't exist on macOS — without this job I'd have had no safe way to find that out
short of breaking a release.

## Work Log

2026-07-11 — Filed the moment CPE-041 showed macOS Rust was uncompiled by CI despite being shipped.
2026-07-11 — Matrix now ubuntu + windows + macos; all green. Closing as Done.

## Notes
Prerequisite for safely writing any platform-gated Rust.
