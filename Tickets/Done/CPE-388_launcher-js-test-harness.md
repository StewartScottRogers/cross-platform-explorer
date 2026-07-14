---
id: CPE-388
title: "AI Console: launcher JS test harness (close the frontend test gap)"
type: Test
status: Done
priority: High
component: Frontend
tags: [ready]
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The AI Console's launcher UI (launcher.html JS) had ZERO automated tests and can't be verified
headlessly — the source of the recent user-facing issues (scrollbar confusion, consent dead-end,
save-set confusion). Add a jsdom harness that loads the real launcher script with mocked
xterm/fetch and unit-tests the rendering + wiring where bugs slip through. Runs under the root
vitest (headless — verifiable in CI, unlike the WebView2 GUI).

## Acceptance Criteria

- [x] A loader mounts launcher.html's DOM + app script in jsdom with stubbed Terminal/FitAddon/
      WebSocket and a mock `fetch`, so functions are callable and DOM assertions work.
- [x] Tests cover: `permHint` (secrets error → actionable), preset dropdown render, `saveSet`
      (empty name guarded; named save POSTs the right body), catalog controls reflect state,
      `stripAltScreen`.
- [x] `npm test` passes; no change to launcher behaviour.

## Work Log
2026-07-14 — Building per the GUI gap analysis (item 1).
