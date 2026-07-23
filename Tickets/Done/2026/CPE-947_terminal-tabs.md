---
id: CPE-947
title: Embedded terminal dock — tab model
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-714
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
First headless slice of the terminal-dock epic (CPE-714). `cpe_server::terminal_tabs`:
- `TerminalDock` over `TermTab { id, title, cwd }` with `open` (auto-title from cwd basename, activates),
  `close` (with active-tab fixup), `activate`, `rename`, `set_cwd` (retitles only if still auto-titled),
  and `active_tab`/`tabs`.

Pure tab bookkeeping; the dock spawns the real shells and renders these tabs.

## Acceptance Criteria
- [x] Open auto-titles + activates; close keeps a valid active tab; close-before-active preserves it.
- [x] set_cwd retitles only an auto-titled tab. 4 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Activated CPE-714 with the tab model. The PTY/shell spawning, terminal rendering
  (xterm), and dock layout are the remaining children. This completes activation of every dormant epic.
