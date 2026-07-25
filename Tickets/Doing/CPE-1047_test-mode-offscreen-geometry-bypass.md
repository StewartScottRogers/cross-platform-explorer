---
id: CPE-1047
title: "Test-mode bypasses the on-screen geometry clamp so automation windows launch truly off-screen"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-616
estimate: 1h
---

## Summary
CPE-1046 makes an automated test window non-focused (won't steal input) + badged, but it's still
**visible**: `geometry::resolve()` (CPE-600) deliberately clamps every window fully on-screen, so
`--x -4000` can't park it off-screen (it's pulled back into view). To fully honor "automated tests never
appear on the user's screen," let **`--test-mode` opt out of the on-screen clamp** so a large-negative
`--x/--y` genuinely positions the window off the visible desktop. Automation (WebDriver) drives the DOM
regardless of position, so off-screen is fully functional; the badge + non-focus remain as belt-and-braces.

## Acceptance Criteria
- [ ] In `--test-mode`, `--x -4000` (or similar off-screen coords) positions the window off the visible
      desktop — the clamp is skipped ONLY in test-mode; normal launches still clamp on-screen (CPE-600
      unchanged).
- [ ] Pure geometry resolver gets a `test_mode`/`allow_offscreen` parameter, unit-tested (clamp applied
      when false, skipped when true); the app passes it from the `--test-mode` flag.
- [ ] The CPE-1045 GUI-smoke harness launches with `--test-mode` + off-screen coords so CI/local runs never
      show a window; `cargo test -p cpe-server` + clippy green.

## Work Log
2026-07-25 — Filed as the CPE-1046 follow-up (the geometry clamp defeats off-screen launch, found during
CPE-1045). Closes the last gap in "automation never touches the user's screen."
