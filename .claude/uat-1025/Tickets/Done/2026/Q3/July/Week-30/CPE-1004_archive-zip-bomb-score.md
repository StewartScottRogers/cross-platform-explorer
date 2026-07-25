---
id: CPE-1004
title: Archive expansion-ratio / zip-bomb safety score
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-1002
---

# CPE-1004 — Archive expansion-ratio / zip-bomb safety score

## Summary

A pure archive expansion-ratio ("zip-bomb") safety score, child of epic CPE-1002 ("File inspection
& safety utilities"). Scores already-gathered per-entry compressed/uncompressed size metadata — no
new dependencies, no extraction, no filesystem I/O. Kept as a standalone module so it doesn't touch
`archive.rs`'s existing multi-format listing plumbing; wiring a real `compressed_size` out of the
`zip` crate (and the other archive backends) into this is a later adapter concern.

New module `crates/server/src/archive_safety.rs`.

## Design

- `pub struct EntrySizes { pub name: String, pub compressed: u64, pub uncompressed: u64 }` — the
  per-entry input the caller supplies (from an archive listing).
- `pub struct FlaggedEntry { pub name: String, pub ratio: f64 }` and
  `pub struct RatioReport { pub total_compressed: u64, pub total_uncompressed: u64, pub overall_ratio: f64,
  pub flagged: Vec<FlaggedEntry>, pub dangerous: bool }`.
- `pub struct RatioLimits { pub max_entry_ratio: f64, pub max_overall_ratio: f64 }` with `Default`
  (`max_entry_ratio: 100.0`, `max_overall_ratio: 100.0`) and a `const fn new(..)` constructor.
- `pub fn expansion_ratio(entries: &[EntrySizes], limits: &RatioLimits) -> RatioReport`: sums
  compressed/uncompressed (saturating, so a crafted/overflowing archive can't panic the sum), computes
  `overall_ratio`, flags (input order) any entry whose own ratio exceeds `max_entry_ratio`, and sets
  `dangerous` when `overall_ratio > max_overall_ratio` OR any entry was flagged.
- **Divide-by-zero rule** (documented on the private `ratio` helper): `uncompressed / compressed`
  where `compressed == 0` is defined as `0.0` if `uncompressed` is also `0` (nothing expanded — an
  all-zero entry is not suspicious), else `f64::INFINITY` (an entry that "expands" from zero bytes is
  the most degenerate/dangerous case there is). Both branches are exact `f64` values — never `NaN`,
  never a panic.
- **Default thresholds** — `max_entry_ratio = 100.0`, `max_overall_ratio = 100.0`: a legitimate mixed
  archive (text/images/already-compressed formats) rarely tops a ~10-20x expansion ratio; 100x is
  generous headroom that still catches pathological single-entry deflate bombs (often 1000x+) without
  false-flagging a well-compressed text/log archive.
- `pub mod archive_safety;` added to `lib.rs` with a doc comment.
- Pure std, zero new dependencies.

## Acceptance Criteria

- [x] A normal archive (per-entry ratios ~2x-10x) scores correct totals + `overall_ratio`, no flags,
  `dangerous == false`.
- [x] A crafted zip-bomb entry (e.g. 1,000 compressed / 10,000,000 uncompressed → 10,000x) is flagged
  and `dangerous == true`.
- [x] Divide-by-zero is handled per the documented rule: zero-compressed + nonzero-uncompressed →
  `f64::INFINITY` (flagged, dangerous, never `NaN`, never a panic); all-zero (0/0) → ratio `0.0`, not
  dangerous.
- [x] `max_entry_ratio` and `max_overall_ratio` are independent: entries that individually pass their
  own threshold can still trip `dangerous` via a tighter `max_overall_ratio`, proving the OR-condition.
- [x] Flag order is deterministic (input order).
- [x] Empty entry list → all-zero totals, `overall_ratio == 0.0`, not dangerous.
- [x] Zero new dependencies; pure over supplied size metadata, no filesystem I/O, no archive
  extraction.
- [x] `pub mod archive_safety;` declared in `lib.rs` with a doc comment.
- [x] `cargo test --lib archive_safety` passes.
- [x] `cargo clippy --all-targets -- -D warnings` clean.
- [x] `cargo clippy --all-targets --features index -- -D warnings` clean.

## Work Log

- 2026-07-24 — Built `archive_safety.rs` end-to-end: `EntrySizes`/`FlaggedEntry`/`RatioReport`/
  `RatioLimits` + `expansion_ratio(entries, limits) -> RatioReport`.
  - **Divide-by-zero rule:** `compressed == 0 && uncompressed == 0` → ratio `0.0` (nothing expanded,
    not suspicious); `compressed == 0 && uncompressed > 0` → ratio `f64::INFINITY` (infinite expansion
    from zero bytes, the most degenerate case — always flagged/dangerous). Both are exact `f64` values;
    the helper can never return `NaN` and never panics, since u64→f64 division only produces `NaN` from
    a `0.0/0.0` float divide, which the explicit zero-guard avoids entirely.
  - **Default thresholds:** `max_entry_ratio = 100.0`, `max_overall_ratio = 100.0` — heuristic; chosen
    as generous headroom above normal mixed-content compression (~2x-10x, and already-compressed
    formats like JPEG sit near 1x) while still well below pathological zip-bomb ratios (single-entry
    deflate bombs commonly reach 1000x+).
  - Totals use `saturating_add` rather than plain `+` so a crafted archive with adversarial size fields
    can't overflow-panic the running sum; a saturated total can only make the resulting ratio look
    *more* dangerous, never mask a real one.
  - `RatioLimits::max_entry_ratio` and `max_overall_ratio` are deliberately independent knobs — proven
    by a test where three 90x entries each pass a 100x entry threshold (nothing flagged) yet still trip
    `dangerous` once the overall threshold is tightened to 50x, exercising the `dangerous` OR-condition
    with an empty `flagged` list.
  - **Epic note:** per the work order this ticket is filed as a child of epic CPE-1002 ("File
    inspection & safety utilities"); no `Tickets/Epics/CPE-1002*.md` brief exists in this repo yet (only
    CPE-1000, a different, already-activated epic about file-type detection). Per the touch-scope for
    this ticket (only `archive_safety.rs`, the one `lib.rs` line, and this ticket file), the epic file
    was not created here — frontmatter still references `epic: CPE-1002` as instructed, matching the
    precedent set by CPE-1001's own scope note.
  - Verified (PowerShell, `crates/server`, `$env:PATH += ";$env:USERPROFILE\.cargo\bin"`):
    `cargo test --lib archive_safety` → 7/7 passed; `cargo clippy --all-targets -- -D warnings` clean;
    `cargo clippy --all-targets --features index -- -D warnings` clean. No clippy fixes were needed.
  - Status → Done; ACs checked; moving to
    `Tickets/Done/2026/Q3/July/Week-30/CPE-1004_archive-zip-bomb-score.md`.
