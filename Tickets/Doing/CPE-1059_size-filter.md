---
id: CPE-1059
title: "Search size filter — cpe_server::size_filter (size: query predicate)"
type: feature
component: Backend
priority: high
status: Doing
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
2026-07-25 (workshift) — Filed by the Product Manager as the first slice of the CPE-703 power-filter query
DSL (fresh headless vein). Independent module; one-line lib.rs `pub mod` at a distinct anchor.
