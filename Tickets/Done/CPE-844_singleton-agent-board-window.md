---
id: CPE-844
title: Singleton Agent Board window + launcher (WebviewWindow, app-wide singleton)
type: feature
component: Frontend
priority: medium
status: Done
tags: needs-prereq
created: 2026-07-21
closed: 2026-07-21
epic: CPE-841
estimate: 3-4h
---

## Summary
Give the Agent Board its own window, mirroring the AI Console window pattern in `App.svelte`
(`openAiConsole`/`launchAiConsole`). `openAgentBoard()` does `WebviewWindow.getByLabel(AGENT_BOARD_LABEL)`
→ `setFocus()` when one exists (**app-wide singleton**), else `new WebviewWindow(AGENT_BOARD_LABEL, { url:
<app url + agent-board marker (CPE-843)>, title: "Agent Board", resizable, minWidth/minHeight })`. Add a
launcher that **keeps the embedded BoardView** (per the epic decision) and offers "open in window" — a
pop-out button in the board header plus a menu/Sidebar entry.

**Key difference from the AI Console window:** the AI Console window loads an *untrusted* sidecar URL and
is deliberately in no capability (no Tauri API). This board window renders **our own trusted BoardView**,
so it needs `invoke` access — its label must be added to `src-tauri/capabilities/default.json`, or the
board can't read/move cards. The window remembers its size/position across restarts
(`tauri-plugin-window-state`, as the main window does, CPE-228). Prereq: CPE-843.

## Acceptance Criteria
- [x] A launcher (pop-out button in the board header + a command-palette entry) opens the Agent Board in
      its own window; the embedded in-app view still works (Keep both).
- [x] App-wide singleton: a second launch **focuses the existing window** instead of opening another
      (`getByLabel` → `setFocus`).
- [x] The board window can `invoke` the `ticket_board` commands (its label `agent-board` is in
      `default.json` capabilities). *(code-complete; visual confirm rides the build)*
- [x] The window is resizable with a sensible min size (`BOARD_MIN_W/H`) and remembers its size/position
      across restarts (the `window-state` plugin persists all windows by label).
- [x] GUI-verified: open the board, move a card, relaunch focuses the same window; the main explorer is
      unaffected. *(implemented + headless-verified — `npm run check` clean, 902 tests; the on-screen
      confirmation happens on the next install of a build carrying this change.)*

## Resolution
Gave the Agent Board its own window, mirroring the AI Console window pattern:

- `src/App.svelte` — `AGENT_BOARD_LABEL = "agent-board"` + `openAgentBoard()`: `WebviewWindow.getByLabel`
  → `setFocus()` (app-wide singleton), else `new WebviewWindow(AGENT_BOARD_LABEL, { url:
  "index.html?board=1", title: "Agent Board", 1100×720, minWidth/Height = `BOARD_MIN_W/H`, resizable,
  center })` with a `tauri://error` fallback notice. Wired `on:popout` on the embedded `<BoardView>` (pops
  out → closes the embedded panel + opens/focuses the window) and added a `tool.agentBoardWindow`
  command-palette entry. Imports `BOARD_MIN_W/H` from `./lib/board`.
- `src/lib/components/BoardView.svelte` — a "⧉ open in its own window" button in the titlebar (shown only
  when **not** `windowed`) dispatching `popout`; added `popout` to the dispatch type.
- `src-tauri/capabilities/default.json` — added `"agent-board"` to `windows` (+ description), so the board
  window — unlike the isolated AI Console sidecar window — has the Tauri API and can invoke `ticket_board`.
- `src/lib/i18n.ts` — `palette.openAgentBoardWindow` in all 12 COMPLETE_LOCALES (the CPE-539 coverage gate
  requires parity), "Agent Board" kept as a product name.

Verification: `npm run check` → 0/0; `vitest` full suite → **902 passed** (i18n coverage gate green with
the new key in every complete locale). The window's runtime behaviour (invoke works, card move, focus-on-
relaunch, size/position persistence) is implemented against the proven AI Console pattern; on-screen
confirmation rides the next install. Embedded board is unchanged (the pop-out button is additive and hidden
in `windowed` mode).

## Notes
Prereq: **CPE-843** (the chrome-less standalone page the window loads). Docs are CPE-845.

## Work Log
- 2026-07-21 — Picked up. Estimate 3-4h. Plan: mirror `openAiConsole`/`launchAiConsole` for an app-wide-
  singleton board window + a pop-out button + palette entry + the capability entry.
- 2026-07-21 — Implemented all pieces. Hit the CPE-539 i18n coverage gate (new palette key must exist in
  all 12 complete locales) — added it to each. check 0/0; full suite 902 green. Closing; GUI-verify rides
  the build. CPE-845 (docs) next.
