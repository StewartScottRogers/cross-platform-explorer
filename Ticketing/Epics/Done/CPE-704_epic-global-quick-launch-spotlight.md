---
id: CPE-704
title: "EPIC: Global quick-launch spotlight overlay"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed: 2026-08-01
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

## Closed 2026-08-01 (sprint) — DoD met
Re-activated and finished this shift. The remaining DoD (global hotkey + overlay window + real
file/action feeding) is built and merged:
- **CPE-1214** — `spotlight_search` + `spotlight_frecent` backbone commands + typed bindings.
- **CPE-1215** — OS-level global hotkey (`tauri-plugin-global-shortcut`) firing `spotlight:open`,
  with a Settings enable/disable + chord control (disabling unregisters the hotkey → no bg cost).
- **CPE-1216** — the Spotlight overlay component (`Spotlight.svelte`): fuzzy-ranked, sectioned
  (Action/Folder/File/Recent), matched-position highlighting, frecency-ordered default view, fed by
  the real streamed file-name walk + the `spotlight_search` command. Opens via the global hotkey OR
  the in-app Command Palette "Spotlight (search everywhere)…" entry.
- **CPE-1219** — gui-smoke pin (`spotlight.smoke.ts`) drives the real built app + captures the
  overlay screenshot for the Visual Critic.

**Gauntlet:** Reviewer APPROVE + UAT PASS + Visual Critic **VISUAL PASS** on the overlay screenshot
(border visible, FILES section hierarchy clean, active-row contrast good, matched run highlighted).
One non-blocking visual nit (underline highlight could pop more) filed as **CPE-1220** (Low).
DoD satisfied: hotkey opens the overlay fast even with the window hidden; it finds + executes
files/folders/actions; disabling unregisters the hotkey.

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** Global hotkey + overlay window + real file/action feeding unbuilt (only fuzzy-score core).
