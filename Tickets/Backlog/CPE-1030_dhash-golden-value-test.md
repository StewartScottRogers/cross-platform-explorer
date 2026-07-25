---
id: CPE-1030
title: Golden-value dHash test to pin bit-order (perceptual.rs)
type: test
component: Backend
priority: low
tags: ready
status: Backlog
created: 2026-07-25
epic: CPE-997
estimate: 30m
---

## Summary
Follow-up captured in the CPE-998 review (epic CPE-997). `perceptual::phash` (dHash) has deterministic and
near-duplicate tests, but they run mostly on all-zero-hash gradient fixtures; only `column_bands` produces
a non-zero hash and **no test pins an exact `u64`**. A bit-order or comparison-direction swap in the hash
packing could therefore go uncaught. Add **one golden-value assertion** on a structured fixture so the
exact bit layout is locked.

## Design
- In `crates/server/src/perceptual.rs` tests, build a deterministic structured image fixture (reuse or
  extend the existing `column_bands` / gradient helper) whose dHash is non-trivial (has both set and unset
  bits).
- Compute its `phash(...)` once, then assert it equals a **hard-coded `u64` literal** (the golden value).
  Include a short comment explaining the fixture and that changing the packing/bit-order intentionally
  requires updating the constant.
- Keep it a pure unit test — no new fixtures on disk, no new dependency.

## Acceptance Criteria
- [ ] A new test asserts `phash(<structured fixture>) == 0x<GOLDEN>` (an exact `u64`), and the fixture's
      hash is non-zero with a mix of set/unset bits (so it actually constrains bit order).
- [ ] `cargo test -q perceptual` passes; `cargo clippy --all-targets -- -D warnings` clean.

## Notes
Test-only change to `perceptual.rs`; touch nothing else. Determine the golden value by running the test
once and reading the actual hash, then pin it (standard golden-test bootstrap) — but sanity-check the
fixture genuinely has mixed bits first so the constant is meaningful.
