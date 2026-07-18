---
id: CPE-681
title: FileSystemProvider abstraction + Local/Fake providers
type: refactor
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-616
estimate: 4h+
---

## Summary
Child of CPE-616. Define a `FileSystemProvider` trait (list/stat/read/write/mkdir/delete), a
`LocalProvider` wrapping today's fs commands, and a `FakeProvider` (in-memory) for tests; route the
existing local FS commands through the provider so a location's scheme (CPE-680) selects the backend.
Prereq: CPE-680.

## Acceptance Criteria
- [x] `FileSystemProvider` trait + `LocalProvider` + `FakeProvider`; the abstraction is documented.
- [~] Existing local FS behaviour is unchanged (all tests pass, GUI-verified); provider unit-tested with the fake.
- [x] `npm run check` + suite green; clippy clean both modes.

## Deferred/Notes
Large refactor of the FS command layer — do **attended** with GUI verification.

## Work Log
2026-07-18 (dayshift) — Picked up (prereq CPE-680 landed). Scoping to the safe headless core (trait + Local + Fake + tests); the risky 'route existing commands through it' rewiring carves out to CPE-685 (attended). No questions.

## Resolution
New pure module `src-tauri/src/provider.rs`: a `FileSystemProvider` trait (list/stat/read/write/mkdir/
delete over provider-relative paths, string errors), a `LocalProvider` over `std::fs`, and an in-memory
`FakeProvider` (mkdir-p semantics; direct-children listing; recursive delete) as the reference impl + test
double. `#![allow(dead_code)]` keeps it compiled + tested until consumers wire it. 3 cargo tests (fake
round-trip, recursive delete, LocalProvider-against-scratch contract); 129→ backend tests pass; clippy
clean both feature modes. Satisfies the epic DoD gate "the abstraction is documented and unit-tested with
a fake provider."

Carve-out: the "route existing commands through the provider" rewiring (the risky, GUI-verified part) is
deferred to [[CPE-685]] (attended). No frontend change (suite unchanged, last green 675). Files:
src-tauri/src/provider.rs, src-tauri/src/lib.rs (mod).
