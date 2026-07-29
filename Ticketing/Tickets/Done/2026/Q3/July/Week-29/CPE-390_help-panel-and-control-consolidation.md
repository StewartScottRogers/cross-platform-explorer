---
id: CPE-390
title: "AI Console: '?' Help panel + consolidate catalog controls into 'Manage agents ▾'"
type: Feature
status: Done
priority: High
component: Frontend
tags: [ready]
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The console grew many controls with no in-app guidance (user: "I am so damned confused"). Added a
task-oriented **"?" Help panel** and **consolidated the advanced catalog controls** (Update / Auto /
Pin / Reset) behind a single **"Manage agents ▾"** dropdown, so the default row is simpler
(agent → provider → model → Keys… / Recent… / Preset / Launch).

## Acceptance Criteria

- [x] "?" opens a Help overlay: task-oriented sections (start an agent, use a key, presets vs keys,
      scrollback, manage agents, recent, permissions). Standard visible border; ×/backdrop close.
- [x] Update agents / Auto / Pin / Reset moved into a "Manage agents ▾" menu (same ids → wiring
      unchanged); menu toggles, closes on outside-click + after an action.
- [x] Harness tests (CPE-388): Help opens/closes; menu toggles + holds the moved controls + closes
      on action. 8 launcher tests + full suite pass; ai-console builds; launcher JS syntax OK.

## Work Log
2026-07-14 — Built per the user's help-system request (recommended option), + the consolidation pass.
