---
id: CPE-713
title: "EPIC: Tray resident — system tray & background quick-access"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
An optional system-tray / menu-bar presence with pinned folders, recent locations, and quick actions, so
CPE can live in the background and open a location instantly.

## Why
Power users keep a file manager one click away. A tray presence with a jump-list of favourites and
launch-on-login turns CPE into an always-available utility — additive and fully disable-able.

## Rough scope (areas, not child tickets)
- Tray icon + menu per OS (Tauri tray API).
- Menu content: pinned folders, recent locations, "new window", quick actions.
- "Close to tray" and "launch on login" options.
- Full opt-out that preserves the delete-test (no tray, no background process when off).

## Open questions (resolve at activation)
- Default off; interaction with app lifecycle (last window closed vs. quit).
- Launch-on-login registration per OS and its uninstall.
- Overlap with the spotlight overlay ([[CPE-704]]) for "open a location fast".

## Definition of Done
- An opt-in tray icon exposes pinned/recent folders and quick actions.
- Close-to-tray and launch-on-login work per OS and are cleanly reversible.
- With the feature off, no tray icon or background residency exists.
