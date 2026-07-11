---
id: CPE-028
title: CI must compile the Rust backend on Windows, not just Linux
type: Test
status: Open
priority: High
component: CI
estimate: 30m
created: 2026-07-11
closed:
---

## Summary

The backend CI job only runs `cargo check` on **ubuntu**. Any code behind `#[cfg(windows)]`, or any
Windows-target-only dependency, is therefore never compiled by CI — it would only fail during a
release build, after a tag is pushed.

This became blocking with CPE-025: correctly resolving Windows known folders requires reading the
registry, which needs a Windows-only crate. Without a Windows CI job that dependency would be
completely unverified.

## Acceptance Criteria

- [ ] CI runs `cargo check`/`clippy`/`test` on windows-latest as well as ubuntu
- [ ] Windows-only code and target-gated dependencies are compiled by CI
- [ ] A deliberate error in `#[cfg(windows)]` code fails CI

## Resolution
## Work Log
## Notes

Prerequisite for CPE-025's real fix.
