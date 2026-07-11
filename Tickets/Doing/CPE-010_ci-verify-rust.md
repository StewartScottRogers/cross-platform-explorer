---
id: CPE-010
title: Verify the Rust backend in CI (cargo check, clippy, tests)
type: Test
status: Open
priority: Critical
component: CI
estimate: 1h
created: 2026-07-11
closed:
---

## Summary

Rust is not installed on the dev machine, so backend changes cannot be compiled or tested locally —
they would only fail during a release build, after a tag is already pushed. CI must compile and test
the Rust side on every push so backend work is verifiable.

## Acceptance Criteria

- [ ] CI job installs Linux webkit deps and runs `cargo check` on `src-tauri`
- [ ] CI runs `cargo clippy -- -D warnings`
- [ ] CI runs `cargo test` (Rust unit tests)
- [ ] At least one Rust unit test exists
- [ ] A deliberate Rust error would fail CI

## Resolution
## Work Log
## Notes

Blocking prerequisite for CPE-011 and any other backend ticket.
