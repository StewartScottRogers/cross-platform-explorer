---
id: CPE-410
title: Session output ("Full output") panel live-updates while open
type: feature
priority: medium
estimate: S
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [ui, ai-console, agent-watch]
---

## Value
CPE-409 refreshed the panel on each open; this keeps it live WHILE open — new console output streams
in without a close/reopen, so you can watch the session as it runs.

## Done
- launcher.html: after the initial replay, poll the ring every 1.2s (reusing fetchSessionOutput,
  no-store) and append ONLY the new bytes; follow the bottom only if the user is already there, and
  leave their scroll position alone if they've scrolled up. A shrunk ring (bounded buffer dropped
  old bytes) triggers a clean redraw. Poll starts on open, stops on close (no leak).
- Harness: FakeTerm gains rows/reset; 2 tests (open loads + starts poll, close stops it; no-active
  guard). 15 launcher tests pass.

- [x] Panel streams new output live while open
- [x] Doesn't yank the user when scrolled up; follows when at bottom
- [x] Poll torn down on close
