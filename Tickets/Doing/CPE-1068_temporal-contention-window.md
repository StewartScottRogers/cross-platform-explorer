---
id: CPE-1068
title: "Temporal contention window — ai_console::conflict_window (same-path touches within a window)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-730
estimate: 2-3h
---

## Summary
Child of CPE-730 (Multi-agent conflict radar). Flag a path where two different agents touch it within a time
window — turning time-agnostic overlap into a live-window contention signal. **Pure fold** in the sidecar
`ai-console` crate, `cargo test` on the 3-OS Sidecar-platform CI — no GUI, no user resource, no new deps.
Does NOT modify `conflict.rs`.

## Design (buildable)
New module `sidecar/ai-console/src/conflict_window.rs`, registered `pub mod conflict_window;` in
`sidecar/ai-console/src/lib.rs` **immediately after `pub mod cost;`**. Mirror the derive/style of
`conflict.rs`.

```rust
pub struct TouchEvent { pub agent: String, pub path: String, pub ts_ms: u64 }
pub struct WindowConflict { pub path: String, pub agents: Vec<String> }

pub fn contended_within(events: &[TouchEvent], window_ms: u64) -> Vec<WindowConflict>;
```
Group events by `path`; sort each group by `ts_ms`; flag the path when two DIFFERENT agents touch it with a
time gap ≤ `window_ms`. Collect the distinct contending agents (sorted). Results sorted by `path`.

## ⚠ Checked arithmetic — MANDATORY (a reviewer caught overflow-panic bugs this shift)
ALL time-gap math via `u64::saturating_sub` — must NOT panic or wrap on `ts_ms == u64::MAX`,
`window_ms == u64::MAX`, or reversed/unsorted timestamps. A fuzzy `[0, u64::MAX]` mix must return cleanly
(add a test). No recursion.

## ⚠ Cross-OS
`ts_ms` is a plain integer; paths string-compared — no `std::path`, no `#[cfg]` assertion.

## Acceptance Criteria
- [x] Two different agents inside the window on the same path → flagged (agents sorted).
- [x] Same two just OUTSIDE the window → not flagged; same-agent rapid touches → not flagged.
- [x] Out-of-order input handled (sorted internally); single event → empty; `u64::MAX` ts/window → no panic.
- [x] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-730 slice (answers the epic's "define conflict
precisely — temporal window" question). Independent module; distinct lib.rs anchor. Saturating-arithmetic
requirement per this shift's reviewer-caught overflow bugs.

2026-07-25 (workshift, Worker) — Implemented `sidecar/ai-console/src/conflict_window.rs`:
`TouchEvent`/`WindowConflict` + `contended_within`. Groups by path (BTreeMap for stable path order),
sorts each group by `ts_ms`, then walks adjacent pairs (`windows(2)`) in sorted order — a different-agent
pair within `window_ms` adds both agents to that path's contender set (BTreeSet, so sorted+deduped).
Gap math: `a.ts_ms.saturating_sub(b.ts_ms).max(b.ts_ms.saturating_sub(a.ts_ms))` — symmetric saturating
subtraction, so it can't underflow/panic regardless of which side is larger or how the timestamps sort.
Registered `pub mod conflict_window;` in `lib.rs` immediately after `pub mod cost;` per the anchor
instruction; did not touch `conflict.rs`. No new dependency — only `std::collections`.

Assumption: the ticket's design implies pairwise adjacency (sorted-by-time neighbours) is the contention
test, rather than an O(n²) all-pairs check per path — this matches "flag when two different agents touch
it with a time gap ≤ window" for the common case and keeps the fold linear per path after the sort. With
3+ agents on one path, contention chains transitively through adjacent pairs (see
`three_agents_on_one_path_collects_all_contending_pairs`), which reads as the intended behavior for a
live-window signal (each successive touch is close to its neighbour) even though it means agents whose
own pairwise gap exceeds the window can still end up in the same result via a chain of closer neighbours.

11 new unit tests added covering: inside/at/outside window boundaries, same-agent rapid touches not
flagged, out-of-order input, single event, empty input, multi-agent chaining, path-sort ordering, and two
`u64::MAX`-focused tests (a direct 0/`u64::MAX` pair at `window_ms = u64::MAX` and `= 0`, plus a fuzzy
multi-path 0/`u64::MAX` mix exercised across several window values including `u64::MAX`) — none panic.

Verify (from `sidecar/ai-console`): `cargo test` → 320 passed, 0 failed, 2 ignored (11 of them the new
`conflict_window` tests). `cargo clippy --all-targets -- -D warnings` → clean, no warnings. No new deps
added to `Cargo.toml`.

Branch `cpe-1068-conflict-window` pushed; PR opened. Ticket left in `Doing` pending PR review/merge per
repo convention (moved to `Done` on merge, not on PR open).
