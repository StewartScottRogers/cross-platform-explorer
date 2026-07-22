---
id: CPE-441
title: "Remove the AI Console 'Full output' (scrollback) panel"
type: Chore
status: Done
priority: Medium
component: Backend
tags: [ready]
created: 2026-07-15
estimate: 1h
closed: 2026-07-15
---

## Summary
User request: remove the AI Console launcher's "Full output" (session scrollback) panel - it does not
work. Removes the button, overlay, its JS + CSS + help text, and the jsdom tests for it. Keeps the
live terminal's own scrolling intact (the shared thumb helpers stay if the main terminal uses them).

## Acceptance Criteria
- [x] The "Full output" button, #scrollback-overlay, its JS (open/show/close/fetchSessionOutput/
      ensureSbWired), CSS, and help text are gone.
- [x] The live terminal still works (no dangling references); launcher jsdom tests updated + green.
- [x] The now-unused backend /api/session/{id}/output route is removed IFF nothing else uses it.
- [x] npm run check clean; JS + Rust suites green.

## Work Log
2026-07-15 - User: remove Full output, it does not work. Note it was recently worked on (CPE-408/409/410).

2026-07-15 - Done. Removed the 'Full output' scrollback overlay from launcher.html: the button, #scrollback-overlay markup, CSS, help section, and the overlay JS (openScrollback/closeScrollback/showScrollback/fetchSessionOutput/ensureSbWired/sbState/base64ToBytes/stripAltScreen) + its 3 event listeners + the activate() display line. Removed the now-unused backend GET /api/session/{id}/output route (handle_session_output) + its test. Removed 3 jsdom tests + 1 backend test. KEPT the live terminal's own custom scrollbar (updateThumb/scheduleThumb/wireScrollbar + per-pane sb/thumb) and the xterm scrollback buffer. Verified: launcher jsdom 14, npm test 420, ai-console --lib 146, npm run check 0/0, clippy clean.

## Resolution
Feature removed cleanly across frontend + backend + tests; the live terminal is unaffected. It was recently worked on (CPE-408/409/410) but the user reported it non-functional and asked for removal.
