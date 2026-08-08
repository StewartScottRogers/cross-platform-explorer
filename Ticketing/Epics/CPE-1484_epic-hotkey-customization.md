---
id: CPE-1484
title: "EPIC: Hotkey customization — view & remap every keyboard shortcut"
type: Task
status: Proposed
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (workshift PM, from the superfile reference pass — see [[superfile-pm-reference]]).**
> **Dormant brief — not decomposed until activated** via `/ticketing-epic activate CPE-1484`.

## Why (the #1 genuine gap superfile surfaced)
superfile ships fully-remappable hotkeys (`hotkeys.toml`, https://superfile.dev/configure/custom-hotkeys/).
CPE has **zero** shortcut-rebinding today — every key is hardcoded. This is one of the most-requested
power-user features in any file manager and the single clearest capability superfile has that CPE lacks
entirely. It's cleanly additive: **zero cost to the fast/small/predictable core when using defaults**, so it
respects PURPOSE.md's tiebreaker.

## Goal
Let users view every keyboard shortcut (global + contextual) in one place and rebind them, with conflict
detection and import/export of a keymap.

## Rough slices (decompose just-in-time when activated)
- A **keymap store / data model** (default map + user overrides, persisted in settings) with a single source
  of truth that the existing shortcut handlers read from instead of hardcoded keys.
- A **conflict detector** (no two actions bound to the same chord in the same scope).
- A **Settings → Keyboard** panel: searchable list of all actions + their current binding, a rebind-capture
  control (press-to-set), reset-to-default, and **import/export** of the keymap.
- Migrate existing hardcoded handlers to read the store (the bulk of the work; do it incrementally by scope).

## Notes
Almost entirely frontend; no new backend surface. Additive/opt-in. Put the controls in Settings (per
[[avoid-modal-permission-popups]] / Settings-home convention). Ship its docs page per CPE-579. Source:
superfile custom-hotkeys docs.
