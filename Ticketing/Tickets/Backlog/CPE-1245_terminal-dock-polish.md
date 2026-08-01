---
id: CPE-1245
title: "Terminal dock polish: orphaned-tab-on-open-error + tab-close a11y + follow-nav busy note"
type: Task
priority: Low
component: frontend
tags: [ready]
created: 2026-08-01
epic: CPE-714
closed:
---

## Context
Non-blocking nits from the CPE-1243 (#545) review:
1. **Orphaned dock tab / xterm on open error** — `addTab()` calls `terminal_dock_open` BEFORE `open_pty`;
   if `open_pty` throws, the tab isn't pushed to `tabs[]`, so the created dock entry + the freshly
   `new Terminal()`'d xterm instance are never closed/disposed (bookkeeping/instance debt, not a live-PTY
   leak). Fix: on `open_pty` failure, roll back the `terminal_dock_open` entry + dispose the xterm.
2. **Tab-close control not keyboard-reachable** — the tab's inline "×" has `tabindex="-1"`. Make it
   keyboard-focusable/operable (or provide a keyboard path to close a tab).
3. **(Note, not necessarily a fix)** follow-nav `cd` injection has no "shell is busy" guard — if the
   user is mid-command / running a foreground program when navigation fires, the `cd` keystrokes go to
   that program. Inherent to the type-cd-vs-respawn design; consider only sending `cd` when the shell
   appears at a prompt, or leave documented as-is.

## Acceptance criteria
- Open-error path leaves no orphaned dock tab or undisposed xterm instance (test the failure path).
- Tab-close is keyboard-reachable/operable.
- Decide + document the follow-nav-while-busy behavior.
