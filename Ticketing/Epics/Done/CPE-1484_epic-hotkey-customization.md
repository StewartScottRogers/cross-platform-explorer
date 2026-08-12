---
id: CPE-1484
title: "EPIC: Hotkey customization — view & remap every keyboard shortcut"
type: Task
status: Done
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (sprint PM, from the superfile reference pass — see [[superfile-pm-reference]]).**
> **Activated 2026-08-10 (sprint PM, bench refill)** — decomposed into four children:
> - CPE-1547 — keymap action registry + persisted override store (foundation, inert plumbing)
> - CPE-1548 — Settings → Keyboard shortcuts viewer dialog (read-only, searchable)
> - CPE-1549 — press-to-set remap capture + live conflict warning + reset-to-default
> - CPE-1550 — import/export keymap via clipboard JSON
>
> Migrating `App.svelte`'s hardcoded `handleKeydown` branches to actually consult the new store is
> deliberately deferred to a **future** batch (not one of the four above) — it's the bulk of the work per
> this brief's own notes, and doing it now would mean multiple tickets editing the same 7300-line handler
> concurrently. CPE-1547-1550 ship the full view/remap/import-export surface as an opt-in layer with zero
> effect on the default (unmigrated) key-handling path, matching PURPOSE.md's "zero cost to the core when
> using defaults."

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

## Closed 2026-08-10
Headless scope complete: CPE-1547 (keymap registry+overrides #770), CPE-1548 (searchable viewer #772), CPE-1549 (remap capture+conflicts+reset #773), CPE-1550 (import/export via clipboard #774). Plus follow-up CPE-1551 (Ctrl+Shift+F shadow fix). Users can view/search/remap/reset/import/export shortcuts; remaps persist. DEFERRED: the handleKeydown→keymap migration (making remaps actually change behavior) — a future batch.
