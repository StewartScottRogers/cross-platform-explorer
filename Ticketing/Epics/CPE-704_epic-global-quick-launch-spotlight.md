---
id: CPE-704
title: "EPIC: Global quick-launch spotlight overlay"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
A system-wide hotkey that pops a lightweight overlay to fuzzy-find files, folders, and CPE actions from
anywhere — even when the main window is hidden or the app is in the tray.

## Why
The fastest way to reach a file is not to navigate to it. A Spotlight/Alfred-style launcher makes CPE a
system-level tool, and it is the natural front-end for the instant index ([[CPE-703]]).

## Rough scope (areas, not child tickets)
- Per-OS global hotkey registration (Tauri global-shortcut plugin).
- A minimal, always-fast overlay window (separate lightweight webview) with fuzzy match + action results.
- Backing query over `find_files_by_name_stream` / the instant index if present.
- Actions surface: recent folders, favourites, and command-palette verbs, not just files.

## Open questions (resolve at activation)
- Overlay as a second window vs. reusing the main window; startup latency budget for the overlay.
- Default hotkey per OS and conflict handling; opt-in vs. on by default.
- Dependency on [[CPE-703]] for speed, or ship on the folder-scoped finder first?

## Definition of Done
- A global hotkey opens the overlay in well under a second, even with the main window hidden.
- The overlay finds files/folders/actions and executes them (open, reveal, run action).
- Disabling the feature unregisters the hotkey and adds no background cost.

## Work Log
2026-07-23 (dayshift) — **Activated.** First slice: **CPE-937** — `spotlight::fuzzy_score` + `rank`: the
pure fuzzy-match/ranking core the overlay lists results with. Remaining: the system-wide hotkey, the
lightweight overlay window, and feeding real files/folders/actions in.
