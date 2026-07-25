---
id: CPE-014
title: Navigation toolbar — back / forward / up / refresh with history
type: Feature
status: Done
priority: High
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Explorer's toolbar has back, forward, up, refresh, an address breadcrumb, and a search box. We only
have up/home. Add a real history stack.

## Acceptance Criteria

- [x] Back/forward buttons driven by a history stack, disabled when unavailable
- [x] Up navigates to parent; Refresh reloads the current directory
- [x] Navigating to a new path truncates the forward history (standard behaviour)
- [x] History logic is a pure, unit-tested module
- [x] Address breadcrumb restyled to Win11 (chevron separators)

## Resolution

Wrote `src/lib/history.ts` as a pure, immutable module (`visit`/`back`/`forward`/`canGoBack`/
`canGoForward`/`current`) and unit-tested it (7 tests) before wiring any UI. Two behaviours worth
calling out, both tested:
- **Forward truncation**: going back and then navigating somewhere new discards the old forward
  entries, as every browser and Explorer does.
- **Refresh is a no-op in history**: re-visiting the current path returns the same object, so hitting
  refresh (or re-clicking the current folder) doesn't pile up duplicate history entries.

NavToolbar has Back/Forward (disabled when unavailable), Up, Refresh, the address breadcrumb with
chevron separators, and the search box. Bound Alt+Left / Alt+Right / F5 / Backspace.

Each tab owns its own `History`, so back/forward are per-tab (see CPE-022).

## Work Log

2026-07-11 — Picked up. Wrote history.ts as a pure module first, with 7 unit tests, before touching UI.
2026-07-11 — Covered forward-truncation and the refresh no-op explicitly — both are easy to get wrong and invisible without tests.
2026-07-11 — Wired Alt+Left/Alt+Right/F5/Backspace. Up at a drive root falls back to Home rather than dead-ending. Closing as Done.

## Notes
