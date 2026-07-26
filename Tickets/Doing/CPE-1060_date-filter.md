---
id: CPE-1060
title: "Search date filter — cpe_server::date_filter (date:/modified: query predicate)"
type: feature
component: Backend
priority: high
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-703
estimate: 3-4h
---

## Summary
Child of CPE-703 (Instant index search). Add a **pure, dependency-free** parser + predicate for date/age
expressions so search can filter by modified time. Backend-only, `cargo test` on the 3-OS matrix — no GUI,
no user resource, no new deps. Standalone module — does NOT touch `index_query.rs`.

## Design (buildable)
New module `crates/server/src/date_filter.rs`, registered `pub mod date_filter;` in `lib.rs` **immediately
after `pub mod name_search;`**. Work entirely in **epoch seconds, UTC** — NO timezone crate, NO
`SystemTime::now()` inside the pure fn (inject `now`), so tests are deterministic and cross-OS-identical.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum DateFilter {
    Before(i64), After(i64),          // epoch-second thresholds derived from the expression
    Between { lo: i64, hi: i64 },     // inclusive span (e.g. a whole year/month/day)
}

/// Parse relative (`<7d`,`>1w`,`today`,`yesterday`,`<30d`,`>1m`,`<1y`) and absolute
/// (`2024`, `2024-07`, `2024-07-25`) + ranges, resolved against `now_s`. Garbage → None.
pub fn parse(token: &str, now_s: i64) -> Option<DateFilter>;
pub fn matches(f: &DateFilter, mtime_s: i64) -> bool;
```
- Relative units: `d`=day(86400), `w`=7d, `m`=30d, `y`=365d (document these fixed approximations). `<7d` =
  modified within the last 7 days → `After(now - 7*86400)`. `today` = the current UTC calendar day span;
  `yesterday` = the prior day span (compute day boundaries by integer floor of `now_s/86400`).
- Absolute: `2024` → the whole-year span; `2024-07` → that month; `2024-07-25` → that day. Compute UTC span
  bounds with plain civil-date→epoch integer math (implement a small days-from-civil helper — no chrono).
- Reject malformed dates (month 13, day 32) and garbage → None (no panic).

## Acceptance Criteria
- [ ] Relative windows correct against a FIXED injected `now` (e.g. `<7d` matches mtime = now-3d, not now-10d).
- [ ] Absolute `2024` / `2024-07` / `2024-07-25` span containment correct (start inclusive, end inclusive of
      the last second of the period or documented half-open — pin it in a test).
- [ ] `today`/`yesterday` boundaries correct via integer day math; malformed dates → None; garbage → None.
- [ ] Deterministic + timezone-free (UTC epoch only); `cargo test -p cpe-server` green; clippy both modes
      clean; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-703 DSL slice. Independent module; `now` is
injected for deterministic tests. One-line lib.rs `pub mod` at a distinct anchor.
