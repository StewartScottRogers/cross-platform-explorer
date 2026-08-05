---
id: CPE-1338
title: "Fold read_model_info into the cross-cutting parser panic-safety fuzz harness"
type: test
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-05
epic: CPE-1002
---

## Summary
`crates/server/tests/parser_panic_safety.rs` (CPE-1169) is the cross-cutting proof that every byte-parser
entrypoint in `cpe-server` handles the full adversarial battery (empty, 1-byte, truncated-at-every-boundary,
all-zeros, all-0xFF, seeded-pseudo-random, valid-magic-then-garbage, overflowing-length) via `catch_unwind`
without panicking. The new 3D parser `cpe_server::model_3d::read_model_info` (STL/OBJ/glTF/GLB, +PLY once
CPE-1337 lands) is NOT yet in it — a gap the CPE-1337 reviewer flagged. Fold it in so the 3D parser's
panic-safety is pinned by the shared harness, not only its inline unit tests.

## Build
- Add `read_model_info` (from `cpe_server::model_3d`) to `crates/server/tests/parser_panic_safety.rs` following
  the EXACT existing pattern (read the file's other entrypoint `#[test]`s + `tests/common/mod.rs`'s
  `assert_no_panic` / `run_battery`): a `#[test]` that runs the full battery through `read_model_info` asserting
  no panic on any adversarial input, and the empty-input case against its documented graceful sentinel (`None`).
- `read_model_info` takes `&[u8]` and returns `Option<ModelInfo>` — the empty/garbage sentinel is `None`. Do NOT
  assert `None` for every adversarial class (the harness deliberately only asserts the empty case + never-panics
  for the rest, because some inputs legitimately parse — e.g. an all-0xFF or magic-collision case; match the
  harness's documented stance).
- Test-only. No production code change. If CPE-1337 (PLY) has merged by the time you branch, great; if not, the
  harness still exercises STL/OBJ/glTF/GLB — either way it's valid (do NOT depend on PLY specifically).

## Acceptance criteria
- `parser_panic_safety.rs` has a `read_model_info` entrypoint test running the full battery with no panic.
- `cargo test -p cpe-server` green (from `crates/server` if `-p` fails); `cargo clippy --all-targets -D warnings`
  clean. No new deps. No production/bindings change.

## Notes
- BACKEND test-only — 3-OS CI (no bindings change → no drift concern). INDEPENDENT of the model_3d.rs format
  lane (CPE-1337 PLY / CPE-1339 glTF geometry) — touches only the test harness file, so it can land in parallel.
- Reference: the existing entrypoint tests in `parser_panic_safety.rs` + `tests/common/mod.rs`.
