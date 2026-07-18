---
id: CPE-678
title: Commander keybindings (copy/move to the other pane)
type: feature
component: Frontend
priority: low
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-617
estimate: 2-3h
---

## Summary
Child of CPE-617. Total-Commander-style keybindings in dual-pane: F5 copies the active pane's selection
to the other pane, F6 moves it, plus swap-panes and mirror-path — all routed through the transfer queue
(CPE-613). Prereq: CPE-677.

## Acceptance Criteria
- [ ] F5/F6 copy/move the active selection to the opposite pane via the transfer manager.
- [ ] Swap panes + mirror the other pane's path work; keys are documented.
- [ ] `npm run check` + suite green.

## Work Log
