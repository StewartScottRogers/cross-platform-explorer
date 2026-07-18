---
id: CPE-685
title: Route local FS commands through the provider abstraction
type: refactor
component: Backend
priority: low
status: Deferred
tags: deferred-internal
created: 2026-07-18
epic: CPE-616
estimate: 3-4h
---

## Summary
Carved out of CPE-681. Rewire the existing Tauri FS commands (list_dir, copy/move/delete, mkdir, read,
stat, …) to go through a `FileSystemProvider` selected by the location's scheme (CPE-680), with
`LocalProvider` (CPE-681) as the local backend — so a remote provider can later slot in. Deferred:
touching the whole command layer is a large refactor whose "byte-for-byte unchanged local behaviour" gate
needs live GUI verification, unsafe to ship blind.

## Acceptance Criteria
- [ ] Local FS commands dispatch through a provider chosen by scheme; local behaviour byte-for-byte unchanged.
- [ ] All existing backend + frontend tests pass; GUI-verified; clippy clean both modes.

## Deferred
deferred-on: large command-layer refactor requiring attended GUI verification. revisit-when: attended
session, or alongside the first real remote provider (CPE-682).
