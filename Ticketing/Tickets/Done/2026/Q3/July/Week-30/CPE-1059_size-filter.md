---
id: CPE-1059
title: "Search size filter — cpe_server::size_filter (size: query predicate)"
type: feature
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-25
epic: CPE-703
estimate: 2-3h
---

## Summary
Child of CPE-703 (Instant index search). Add a **pure, dependency-free** parser + predicate for human size
expressions, so search can filter by file size. Backend-only, `cargo test` on the 3-OS matrix — no GUI, no
user resource, no new deps. Standalone module — does NOT touch `index_query.rs` (integration into the query
grammar is a deliberately separate later ticket).

## Design (buildable)
New module `crates/server/src/size_filter.rs`, registered `pub mod size_filter;` in `lib.rs` **immediately
after `pub mod index_query;`**.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SizeOp { Gt, Lt, Ge, Le, Eq }
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SizeFilter { Cmp { op: SizeOp, bytes: u64 }, Range { lo: u64, hi: u64 } } // Range inclusive

/// Parse `>1mb`, `<=500k`, `=0`, `1mb..1gb`, `2.5G` → a SizeFilter; garbage → None.
pub fn parse(token: &str) -> Option<SizeFilter>;
pub fn matches(f: &SizeFilter, bytes: u64) -> bool;
```
- Units (ASCII, case-insensitive): `k/kb`, `m/mb`, `g/gb`, `t/tb`, bare number = bytes. **Use 1024-based**
  (1kb = 1024) — pick this and **document + test** it. Accept a decimal mantissa (`2.5g`, `500.5k`) via
  integer math (parse mantissa, multiply, truncate/round — document which; avoid float nondeterminism where
  practical, or use f64 only for the mantissa then cast — pin the exact rounding in a test).
- Range `lo..hi` inclusive on both ends; reject `hi < lo` → None.
- Locale-free: ASCII digits + `.` decimal only; reject anything else → None (no panic).

## Acceptance Criteria
- [ ] Unit math correct + documented (1kb = 1024); `2.5g`/`500k` parse to the right byte counts.
- [ ] Each operator's boundary inclusivity correct (`>=`/`<=` include the endpoint, `>`/`<` exclude).
- [ ] `lo..hi` range inclusive; `hi < lo` → None; garbage/empty → None (no panic).
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (sprint) — Filed by the Product Manager as the first slice of the CPE-703 power-filter query
DSL (fresh headless vein). Independent module; one-line lib.rs `pub mod` at a distinct anchor.

2026-07-25 (sprint, overnight Worker) — Built `crates/server/src/size_filter.rs` per design: `SizeOp`
{Gt,Lt,Ge,Le,Eq} + `SizeFilter` {Cmp,Range} with the `serde::Serialize` + `cfg_attr(specta)` derive stack
matching `code_outline.rs`/`index_query.rs`. `pub mod size_filter;` added immediately after `pub mod
index_query;` in `lib.rs`. Std-only, no new deps, no `#[cfg]`-dependent behavior, operates only on `u64` +
ASCII `&str` (no `std::path`).

Assumptions made (no user available overnight):
- **Units are 1024-based** (k/kb=1024, m/mb=1024², g/gb=1024³, t/tb=1024⁴), case-insensitive suffixes;
  bare number = bytes. Documented in the module doc-comment and pinned by tests.
- **Decimal mantissa rounding**: parse mantissa as `f64`, multiply by the unit's byte count, then
  `f64::round()` (ties away from zero) before casting to `u64`. `2.5g`/`500.5k` land on exact integers
  (power-of-two arithmetic); `1.1k` → 1126 and `1.5b`/`1.4b` → 2/1 are the tests that actually exercise
  rounding.
- **Bare amount with no operator prefix** (e.g. `2.5g`) defaults to `SizeOp::Eq` — the design doc's own
  example list mixes an explicit `=0` with an operator-less `2.5g`, so `Eq` was the only default that
  keeps both consistent.
- Also accept an explicit `b` suffix (bytes) as a harmless superset of the spec's bare-number-only rule;
  doesn't conflict with any listed unit and isn't relied on by any acceptance criterion.
- Range `lo..hi`: both sides parsed as plain amounts (no operator), inclusive on both ends, `hi < lo` →
  `None`, `lo == hi` accepted (single-value "range").

Verification (from `crates/server`, PowerShell with `cargo` added to PATH — no toolchain issue, just a
non-login shell):
- `cargo test` (whole crate): all green, including 18 new `size_filter` unit tests.
- `cargo clippy --all-targets -- -D warnings`: clean, exit 0.
- `cargo clippy --all-targets --features index -- -D warnings`: clean, exit 0.
- `cargo build --features specta`: clean, exit 0 (sanity-checked the `specta::Type` derive path even
  though the ticket only mandated default + `index`).
- No Defender "os error 225" encountered this run.

Does **not** touch `index_query.rs` per the ticket's explicit scope — `size:` grammar wiring is left for
a later ticket. Branch `cpe-1059-size-filter` pushed; PR opened for user review.
