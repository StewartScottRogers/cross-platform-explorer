---
id: CPE-681
title: FileSystemProvider abstraction + Local/Fake providers
type: refactor
component: Backend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-616
estimate: 4h+
---

## Summary
Child of CPE-616. Define a `FileSystemProvider` trait (list/stat/read/write/mkdir/delete), a
`LocalProvider` wrapping today's fs commands, and a `FakeProvider` (in-memory) for tests; route the
existing local FS commands through the provider so a location's scheme (CPE-680) selects the backend.
Prereq: CPE-680.

## Acceptance Criteria
- [ ] `FileSystemProvider` trait + `LocalProvider` + `FakeProvider`; the abstraction is documented.
- [ ] Existing local FS behaviour is unchanged (all tests pass, GUI-verified); provider unit-tested with the fake.
- [ ] `npm run check` + suite green; clippy clean both modes.

## Deferred/Notes
Large refactor of the FS command layer — do **attended** with GUI verification.

## Work Log
