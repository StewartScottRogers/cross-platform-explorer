---
id: CPE-1060
title: "Search date filter — cpe_server::date_filter (date:/modified: query predicate)"
type: feature
component: Backend
priority: high
status: Done
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

2026-07-25 (workshift, overnight Worker) — Built `crates/server/src/date_filter.rs` end-to-end, std-only,
no new deps. Registered `pub mod date_filter;` in `lib.rs` immediately after `pub mod name_search;` as
directed.

**Design decisions / assumptions (none of these were pinned exactly in the ticket, so logging the calls
made):**
- `DateFilter::Before`/`After` boundary rule: pinned as a clean, non-overlapping partition around one
  threshold `t` — `After(t)` is **inclusive** (`mtime_s >= t`, the "recent" side of `<Nunit`), `Before(t)`
  is **exclusive at the same t** (`mtime_s < t`, the "older than" side of `>Nunit`). So `<7d` and `>7d`
  never both match (or both miss) a file modified exactly 7 days ago — it counts as "recent".
- `Between{lo,hi}` (whole absolute spans + `today`/`yesterday`) is inclusive at **both** ends
  (`lo <= mtime_s <= hi`) — pinned + covered by boundary tests (first/last second of the span matches,
  one second outside on either side does not).
- Relative-unit fixed approximations (documented in the module doc-comment): `d`=86400s, `w`=7d=604800s,
  `m`=30d=2,592,000s (approx month), `y`=365d=31,536,000s (approx year, no leap adjustment). A relative
  `<1y` means "within the last 365 days," not "since this calendar date last year" — absolute tokens
  (`2024`, `2024-07`) are the calendar-exact alternative.
- `days_from_civil(y, m, d)` implements Howard Hinnant's public-domain integer civil-date algorithm
  (proleptic Gregorian, valid for any `i64` year, no timezone). Verified against known reference deltas in
  a dedicated test (epoch day 0 = 1970-01-01; 2024 leap-year length 366; 1900 NOT a leap year despite
  `%4==0` — the `%100`/`%400` rule is exercised).
- Absolute-token day/month bounds are validated in two passes: range check (`month` 1-12, `day` 1-31) then
  a `days_in_month(y, m)` check catching Feb 30, Apr 31, and non-leap Feb 29 — all reachable only after the
  first-pass range check, so no panic path exists.
- Relative-token parsing rejects non-digit, empty, and negative counts (`<d`, `<-7d`, `<7.5d` all → `None`)
  and unrecognized unit letters — parser never panics on garbage.
- Standalone module confirmed: no import of / edit to `index_query.rs`; `cargo clippy` clean in both
  default and `--features index` modes confirms no accidental coupling.

**Verification:** `cargo test -p cpe-server` → 825 passed, 0 failed (11 new `date_filter` tests: relative
recent/older windows against a fixed injected `now`, week/month/year unit math, case-insensitivity,
today/yesterday boundary math, absolute year/month/day span containment incl. December→January rollover,
malformed-calendar-date rejection, garbage/edge-case rejection). `cargo clippy --all-targets -- -D
warnings` clean; `cargo clippy --all-targets --features index -- -D warnings` clean. No new dependencies
added to `Cargo.toml`.

Branch `cpe-1060-date-filter`, PR opened against `main`.
