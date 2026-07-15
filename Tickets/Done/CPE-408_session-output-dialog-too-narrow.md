---
id: CPE-408
title: Session output dialog is too narrow and wraps the console
type: bug
priority: medium
estimate: XS
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [ui, ai-console, bug]
---

## Problem
The "Session output" (⇕ Full output) replay panel is only 88vw and fits its terminal
synchronously right after un-hiding — before layout settles — so it measures too few columns and
the console output re-wraps at a narrower width than the live session.

## Fix
- Widen the panel (96vw / 86vh) so it's at least as wide as the live terminal.
- Fit on the next frame (after the overlay lays out), THEN write, so the replay wraps at the
  correct full width.

## Acceptance
- [x] Session output panel uses nearly the full window; long console lines no longer re-wrap
- [x] Fit runs after layout; content written at the fitted width
