---
id: CPE-381
title: "Fix CI: clippy -D warnings (dead-code without feature + test-code lints)"
type: Bug
status: Done
priority: High
component: CI
tags: [ready]
estimate: 30m
created: 2026-07-14
closed: 2026-07-14
---

## Summary

CI (`cargo clippy --all-targets -- -D warnings`, both feature modes) went red across the catalog
work because local `cargo clippy --quiet` checks neither `--all-targets` (test code) nor `-D
warnings`. Two classes: (1) `src-tauri` `keyverify` pure fns are dead code without the
`sidecar-platform` feature; (2) `cloned_ref_to_slice_refs` / `unnecessary_get_then_check` in test code.

## Fix

- `src-tauri`: gate `mod keyverify` behind `#[cfg(feature = "sidecar-platform")]` (nothing uses it
  without the feature; its logic is still built/linted under the feature).
- host + ai-console tests: `&[x.clone()]` → `std::slice::from_ref(&x)`; `.get(k).is_none()` →
  `!.contains_key(k)`.
- Reproduced all four CI clippy invocations locally (host, ai-console, src-tauri ±feature) → all PASS.

## Notes
Lesson saved to memory: run `cargo clippy --all-targets -- -D warnings` (both feature modes) to
match CI — `--quiet` alone misses test-code + dead-code-under-feature.

## Work Log
2026-07-14 — Found CI red on the catalog merges; fixed + reproduced locally.
