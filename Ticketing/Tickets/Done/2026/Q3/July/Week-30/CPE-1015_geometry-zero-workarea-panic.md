---
id: CPE-1015
title: Fix startup panic in window geometry when a monitor reports a zero-size work area
type: bug
component: Backend
priority: high
tags: ready
status: Done
created: 2026-07-24
closed: 2026-07-24
epic: CPE-688
estimate: 30m
---

> **Done (PR #338, merged).** `saturating_sub` applied at `geometry.rs:192-193`; 2 regression tests (zero-width
> + zero-height work area → `Ok`, position clamped to origin) that panic against the old code. Independently
> reviewed (APPROVE), 3-OS CI green.

## Summary
Found by the 2026-07-24 sprint bug-audit (second wave). `crates/server/src/geometry.rs` `resolve()`
**panics at window-launch time** when a monitor reports a zero-width or zero-height work area (a real, if rare,
condition — a display that just disconnected, or a virtual/RDP monitor mid-transition).

Line 180-181 clamp the window size *up* to a floor of 1 (`want_w.min(wa.width.max(1))`), so `width` can exceed
the real `wa.width` (0). Then the off-screen-clamp math does a **non-saturating** `u32` subtraction:
```rust
let max_x = wa.x + (wa.width - width) as i32;   // wa.width=0, width=1  → underflow
let max_y = wa.y + (wa.height - height) as i32;
let x = want_x.clamp(wa.x, max_x);
```
- **Debug / overflow-checks build** (e.g. `cargo test`): `0u32 - 1u32` → panic `"attempt to subtract with overflow"`.
- **Release build**: wraps to `max_x = -1`, then `want_x.clamp(wa.x=0, -1)` → panic `"min > max"` (`Ord::clamp`).

Only the empty-monitors list is currently guarded (`GeometryError::NoMonitors`); a zero-size work area is not.
Verified by the auditor with a standalone repro of both panic paths.

## Fix
Use saturating subtraction for the off-screen-clamp bounds (fixes both the debug underflow and the release
`clamp` min>max, since `max_x`/`max_y` then never fall below `wa.x`/`wa.y`):
```rust
let max_x = wa.x + wa.width.saturating_sub(width) as i32;
let max_y = wa.y + wa.height.saturating_sub(height) as i32;
```

## Acceptance Criteria
- [ ] `resolve()` with a zero-width and/or zero-height work area returns `Ok` (position clamped onto the
      work-area origin) instead of panicking. Add a regression test covering `WorkArea { width: 0, .. }` and
      `{ height: 0, .. }` with default args.
- [ ] Existing geometry tests still pass; normal (non-zero) work-area clamping behaviour unchanged.
- [ ] `cargo test -p cpe-server geometry` green; clippy clean both feature modes; no new deps.

## Notes
Epic: attributed to CPE-688 (explorer robustness) — geometry resolves at window creation. Backend-only,
headless-testable.
