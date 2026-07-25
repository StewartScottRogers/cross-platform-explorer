---
id: CPE-010
title: Verify the Rust backend in CI (cargo check, clippy, tests)
type: Test
status: Done
priority: Critical
component: CI
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Rust is not installed on the dev machine, so backend changes cannot be compiled or tested locally —
they would only fail during a release build, after a tag is already pushed. CI must compile and test
the Rust side on every push so backend work is verifiable.

## Acceptance Criteria

- [x] CI job installs Linux webkit deps and runs `cargo check` on `src-tauri`
- [x] CI runs `cargo clippy -- -D warnings`
- [x] CI runs `cargo test` (Rust unit tests)
- [x] At least one Rust unit test exists
- [x] A deliberate Rust error would fail CI

## Resolution

Added a second CI job, "Backend — cargo check, clippy, test", running on every push/PR:
installs the Linux webkit deps, builds the frontend (tauri's build script needs `dist/`),
then runs `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
Added 5 Rust unit tests to `src-tauri/src/lib.rs`.

This immediately paid for itself: the very first run FAILED on
`clippy::items_after_test_module` — I had placed `#[cfg(test)] mod tests` before `run()`. Under the
old setup that would have gone unnoticed until a release build. Moved the test module to the end of
the file (with a comment explaining why it must stay there) and CI went green.

## Work Log

2026-07-11 — Picked up first: without it, no backend change is verifiable (Rust is not installed locally).
2026-07-11 — Added the backend CI job and 5 Rust unit tests; pushed.
2026-07-11 — CI FAILED on clippy `items_after_test_module`. Exactly the class of error this ticket exists to catch.
2026-07-11 — Moved the test module to the bottom of lib.rs. CI green (check + clippy + test). Closing as Done.

## Notes

Blocking prerequisite for CPE-011 and any other backend ticket.
