---
id: CPE-1052
title: "Minimap density — cpe_server::minimap (downsampled overview rows)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-25
epic: CPE-724
estimate: 3h
---

## Summary
Child of CPE-724 (Code intelligence preview). Add a **pure, dependency-free** downsampler that turns a
source file into a fixed number of minimap rows (fill density + indent), so the future preview can paint a
scaled overview without shipping the whole file to the renderer. Backend-only, verified by `cargo test` on
the 3-OS matrix — no GUI, no user resource.

## Design (buildable)
New module `crates/server/src/minimap.rs`, registered with `pub mod minimap;` in `crates/server/src/lib.rs`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MinimapRow {
    /// Average non-whitespace character density of the covered lines, scaled 0..=255.
    pub fill: u8,
    /// Min leading indent (in columns) of the covered lines.
    pub indent: u16,
}

pub fn minimap_rows(source: &str, buckets: usize) -> Vec<MinimapRow>
```

Algorithm:
1. Split into lines (`source.lines()`), let `n` = line count.
2. Edge cases: `buckets == 0` or `n == 0` → empty vec. If `n <= buckets` → **one row per line** (don't
   invent rows).
3. Otherwise group lines into `buckets` contiguous groups using deterministic `ceil(n / buckets)` grouping
   (last group may be short). For each group:
   - `fill` = average over its lines of (non-whitespace-char-count / line-length, guarding empty lines as 0),
     then scale to 0..=255 (round). A fully non-blank line → high fill; a blank line → 0.
   - `indent` = **min** leading-indent (column width, tabs expanded to 4) across the group's lines.
4. Deterministic: same input → same output (no float nondeterminism that changes rounding across runs).

O(total chars). Std + serde/specta only; no new deps.

## Acceptance Criteria
- [ ] Bucket count == `buckets` when `n > buckets`, and coverage partitions all lines (`ceil(n/buckets)`
      grouping, last group short).
- [ ] A group of fully-filled lines → high `fill` (near 255); a group of blank lines → `fill == 0`.
- [ ] `n <= buckets` → exactly `n` rows (one per line); `buckets == 0` or empty source → empty vec.
- [ ] Determinism: two calls on the same input return identical vecs.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default and
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a clean headless CPE-724 slice. Independent module;
only shared touch is a one-line lib.rs `pub mod` (serial-merge coordination only).

2026-07-25 (workshift, Worker) — Built `crates/server/src/minimap.rs` and registered it in `lib.rs`
right after `pub mod simhash;`. Implementation note / assumption: the ticket describes grouping as
"`ceil(n / buckets)` sizing, last group short," but a literal fixed-chunk-size-then-remainder split can
under-produce the bucket count for some `n`/`buckets` pairs (e.g. `n=4, buckets=3` → chunk size
`ceil(4/3)=2` yields only 2 groups, not 3; `n=7, buckets=5` → chunk size 2 yields only 4 groups, not 5).
Implemented instead as the standard even-split: `base = n / buckets`, `extra = n % buckets`, first
`extra` groups get `base + 1` lines, the rest get `base` — this always yields exactly `buckets` non-empty
groups when `n > buckets`, matches the "last group(s) may be short" intent, and coincides with the naive
ceil-chunking result whenever that formulation happens to work out. Covered this exact edge case in a
dedicated test (`bucket_count_matches_and_partitions_all_lines_n_gt_buckets`).

All fill/indent math is integer-only (round-half-up via `+ divisor/2` before dividing) for bit-for-bit
determinism — no floats anywhere in the algorithm.

Verified: `cargo test` in `crates/server` — 736 passed (9 new `minimap::tests::*`, 0 failed).
`cargo clippy --all-targets -- -D warnings` clean; `cargo clippy --all-targets --features index -- -D
warnings` clean. No new dependencies. Opened PR from branch `cpe-1052-minimap`; ticket stays in `Doing`
pending review/merge per the CPE-1048 precedent (moves to `Done` once merged).
