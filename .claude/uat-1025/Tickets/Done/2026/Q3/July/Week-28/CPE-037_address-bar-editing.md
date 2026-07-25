---
id: CPE-037
title: Editable address bar (Ctrl+L / Alt+D)
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Ctrl+L or Alt+D (or clicking the empty part of the address bar) turns the breadcrumb into a text box
so a path can be typed or pasted.

## Acceptance Criteria

- [x] Ctrl+L / Alt+D focuses and selects an editable path field
- [x] Enter navigates; Escape reverts to breadcrumbs
- [x] A nonexistent path shows a clear error and does not navigate
- [x] Environment variables (e.g. %USERPROFILE%) are expanded
- [x] Clicking a breadcrumb still navigates as before

## Resolution

Ctrl+L / Alt+D — and clicking the blank part of the address bar — swap the breadcrumbs for a text
field seeded with the current path and pre-selected. Enter navigates, Escape reverts.

A typed path is **verified before navigating**: we list it first, and on failure show "Can't find
…" and stay put, rather than navigating into a broken state. `%USERPROFILE%` is expanded. Keydown
inside the field stops propagating so list shortcuts don't fire while typing a path.

## Work Log

2026-07-11 — Typed paths are validated by listing them before navigation, so a typo can't strand the view. Closing as Done.

## Notes
