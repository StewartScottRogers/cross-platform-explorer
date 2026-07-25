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

## Work Log

### 2026-07-25 — CPE-1030 implemented

Added golden-value test `phash_golden_value_column_bands()` using the `column_bands(9, 20, 100, true)` fixture.
Golden hash determined: `0x5555555555555555` (32 set bits, perfect alternating pattern due to the alternating
left>right comparisons across the vertical bands). Test pins the exact bit layout and includes sanity checks
to verify non-trivial mixed bits. All 12 perceptual tests pass; clippy clean. Ready for PR.

### 2026-07-25 — Fixture replaced after review (row-order coverage)

Review (PR #346) flagged that `column_bands` varies only in x: its bands span the full image height, so
after the 9×8 resize all 8 grid rows are pixel-identical (every row's byte = `0x55`). That hash is invariant
under row-reversal, so a row-major / cross-row packing bug would be invisible — the golden constant only
constrained intra-row comparison direction, not the packing order the ticket exists to lock.

Replaced it with a new test-module fixture `diagonal_staircase(180, 160)` that varies in **both x and y**: the
image is 8 horizontal bands (one per output row), each white up to a per-band column then black, so every grid
row has its transition at a distinct column and yields a **distinct** packed byte. Re-derived golden hash:
`0xe0f0f87c3e1f0f07` — bytes `e0 f0 f8 7c 3e 1f 0f 07` (all 8 distinct), 34 set bits. Added assertions that
the 8 bytes are not all equal and that `hash != hash.swap_bytes()` (not row-reversal-symmetric), so row-order
bugs are now detectable. All 12 perceptual tests pass; `cargo clippy --all-targets -- -D warnings` clean.
