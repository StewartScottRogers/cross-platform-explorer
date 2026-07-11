---
id: CPE-028
title: CI must compile the Rust backend on Windows, not just Linux
type: Test
status: Done
priority: High
component: CI
estimate: 30m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The backend CI job only runs `cargo check` on **ubuntu**. Any code behind `#[cfg(windows)]`, or any
Windows-target-only dependency, is therefore never compiled by CI — it would only fail during a
release build, after a tag is pushed.

This became blocking with CPE-025: correctly resolving Windows known folders requires reading the
registry, which needs a Windows-only crate. Without a Windows CI job that dependency would be
completely unverified.

## Acceptance Criteria

- [x] CI runs `cargo check`/`clippy`/`test` on windows-latest as well as ubuntu
- [x] Windows-only code and target-gated dependencies are compiled by CI
- [x] A deliberate error in `#[cfg(windows)]` code fails CI

## Resolution

Converted the backend CI job to a matrix over `[ubuntu-latest, windows-latest]`, gating the apt
step to Linux only. Both now run `cargo check --all-targets`, `cargo clippy -- -D warnings`, and
`cargo test`.

This immediately mattered: CPE-025's real fix needs a Windows-only registry crate, and Linux CI would
never have compiled a line of it. The Windows job passed with the new `winreg` dependency and the
`#[cfg(windows)]` registry tests, so that code is now genuinely verified rather than hoped-for.

## Work Log

2026-07-11 — Filed once CPE-025 revealed that Windows-only code was entirely uncompiled by CI.
2026-07-11 — Matrixed the backend job across ubuntu + windows; gated the apt step to Linux.
2026-07-11 — Windows job green, compiling winreg and running the #[cfg(windows)] registry tests. Closing as Done.

## Notes

Prerequisite for CPE-025's real fix.
