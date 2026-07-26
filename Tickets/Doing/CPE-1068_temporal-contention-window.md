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
- [ ] Two different agents inside the window on the same path → flagged (agents sorted).
- [ ] Same two just OUTSIDE the window → not flagged; same-agent rapid touches → not flagged.
- [ ] Out-of-order input handled (sorted internally); single event → empty; `u64::MAX` ts/window → no panic.
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-730 slice (answers the epic's "define conflict
precisely — temporal window" question). Independent module; distinct lib.rs anchor. Saturating-arithmetic
requirement per this shift's reviewer-caught overflow bugs.
