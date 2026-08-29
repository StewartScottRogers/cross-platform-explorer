---
id: CPE-713
title: "EPIC: Tray resident — system tray & background quick-access"
type: Task
status: In Progress
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

## Work Log
2026-07-23 (dayshift) — **Activated.** First slice: **CPE-946** — `tray_quick::QuickAccess`: the pinned +
recent quick-access list model for the tray menu. Remaining: the tray icon/menu, minimize-to-tray, and
background quick-launch.

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** Tray icon/menu + close-to-tray + launch-on-login unbuilt (only QuickAccess model).

## Closeout audit 2026-08-29 - KEEP OPEN

Both children Done. Platform-neutral (Tauri tray API), but incomplete on all three.

**Shipped:** `tray.rs` builds a real tray - quick-access entries, Show/Hide, Quit, `tray://open-folder` events the frontend listens for, and `note_folder` keeping recents fresh. Close-to-tray is opt-in and default off, with a unit test covering all four combinations of tray-present x setting.

**Three gaps, all named in the DoD:**
1. **Launch-on-login is entirely unbuilt.** No autostart plugin, no `Run` key write, no LaunchAgent - a repo-wide grep returns nothing.
2. **The tray icon is created unconditionally**, so the DoD line "with the feature off, no tray icon exists" cannot hold. There is no off switch.
3. **Pinning a folder to the tray is unreachable.** `tray_quick.rs` has `pin`/`unpin`/`remove` but no command or UI calls them - only `touch` is reachable, so the menu can only ever show recents.

Cost: a settings-gated `setup()`, an autostart plugin with reversible registration, and a pin menu item.
