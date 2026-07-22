---
id: CPE-843
title: Standalone Agent Board page — render BoardView chrome-less for its own window
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-841
estimate: 2-3h
---

## Summary
Foundation for the standalone Agent Board window (epic CPE-841). Let the frontend render **just** the
`BoardView` — no explorer chrome (no menu/nav/command bars, tab strip, sidebar, or status bar) — when the
app is loaded with an `agent-board` marker (a URL hash/query the board window will use). Reuse the existing
`board.ts` model + the `ticket_board` backend unchanged; the standalone page drives the same `invoke`
commands the embedded view does. The normal explorer render path is untouched when the marker is absent.

## Acceptance Criteria
- [x] Loading the app URL with the agent-board marker (`?board`) mounts **only** the BoardView, filling the
      window, with no explorer chrome.
- [x] Without the marker, the app renders the normal explorer exactly as today (no behavioural change).
- [x] The standalone board reads and moves cards via the existing `ticket_board` commands — no backend
      change.
- [x] Marker detection + the standalone-vs-explorer mount decision is unit-tested (vitest); `npm run
      check` clean.

## Resolution
Mirrored the existing `?float` torn-off-preview pattern for a `?board` standalone Agent Board window.

- `src/lib/bootMode.ts` (new) — a pure `bootMode(search)` → `"float" | "board" | "explorer"`, extracted so
  the window-selection decision is unit-testable without a DOM. `src/lib/bootMode.test.ts` (4 tests).
- `src/main.ts` — uses `bootMode(location.search)`: `float` → `FloatPreview` (unchanged, no settings load);
  otherwise load settings/theme, then `board` → the new `AgentBoardApp`, else the full `App`.
- `src/lib/components/AgentBoardApp.svelte` (new) — the standalone window root: mounts `BoardView`
  `windowed` with `root=""`, routes `close` → `getCurrentWindow().close()`; `launch`/`help` are no-ops for
  now (cross-window agent launch is CPE-844, in-window docs CPE-845).
- `src/lib/components/BoardView.svelte` — added a `windowed` prop (default `false`, so the embedded overlay
  is unchanged); when set, the overlay drops its dim backdrop + centring and the panel fills the viewport
  (no border/radius/shadow), so the board *is* the window.

The board is self-contained (loads/moves cards via the existing `ticket_board` commands, remembers its own
project root via `cpe.boardRoot`), so no backend or data wiring was needed. `root=""` falls back to the
persisted board root or the board's own pick-a-folder prompt.

Verification: `npm run check` → 0 errors / 0 warnings; `vitest` full suite → **902 passed** (incl. the 4
new bootMode tests and the unchanged 7 BoardView tests). App explorer path is byte-for-byte unchanged when
`?board` is absent. The window launcher + app-wide singleton + capability entry that actually *open* this
window are CPE-844.

## Work Log
- 2026-07-21 — Picked up. Estimate 2-3h. Plan: mirror the `?float` window pattern with a `?board` marker +
  a chrome-less standalone board root, reusing the self-contained BoardView.
- 2026-07-21 — Found the `?float`/`FloatPreview` precedent in `main.ts` and that BoardView is a modal
  overlay (fixed backdrop + centred panel). Added a pure `bootMode`, an `AgentBoardApp` root, and a
  `windowed` prop to BoardView (fills the window, no backdrop). check clean; full suite 902 green. Closing.
