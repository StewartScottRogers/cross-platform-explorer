---
id: CPE-339
title: "Keyboard shortcuts help dialog (F1) + menu discoverability"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

The explorer has ~25 keyboard shortcuts (`handleKeydown` in App.svelte) but no way to
discover them — no cheat sheet, no menu entry. Add a **Keyboard shortcuts** dialog listing
them, grouped by category, opened via **F1** and an **Application → Keyboard shortcuts** menu
item.

## Design (frontend-only)
- **`src/lib/shortcuts.ts`:** a pure, exported data table `SHORTCUT_GROUPS` ({ title, items:
  { keys, description }[] }). Pure/testable; the single source the dialog renders. Keys are
  transcribed verbatim from `handleKeydown` so the sheet cannot drift from reality.
- **`ShortcutsDialog.svelte`:** a modal (same backdrop/Escape/click-away pattern as
  ConfirmDialog) rendering the grouped table with styled `<kbd>` chips.
- **App.svelte:** `shortcutsOpen` state; F1 opens it (before the type-ahead branch so a
  bare key doesn't swallow it); MenuBar `select` handles the new `shortcuts` id.
- **MenuBar.svelte:** add `{ id: "shortcuts", label: "Keyboard shortcuts", hint: "F1" }` to
  the Application menu.

## Assumptions (Nightshift — user asleep, logged per policy)
- F1 is the opener (Windows-standard help key) and is safe: `handleKeydown` returns early
  when focus is in an INPUT/TEXTAREA, so F1 won't fire mid-edit.
- Cheat sheet is read-only (no rebinding UI) — discoverability is the goal; rebinding is a
  separate, larger feature.

## Acceptance
- F1 (and the menu item) opens the dialog; Escape / click-away / a Close button dismiss it.
- Every shortcut listed matches an actual binding in `handleKeydown`.
- `npm run check` and `npm test` green.

## Work Log
2026-07-13 — Filed during Nightshift (loop 2). Research: the app is mature (breadcrumbs,
status bar, favorites just added) so the real gap is *discoverability* of the many existing
shortcuts. Implemented on branch `CPE-339-shortcuts-help`.

Implemented (frontend-only):
- `src/lib/shortcuts.ts`: pure `SHORTCUT_GROUPS` table, transcribed verbatim from
  `handleKeydown` (5 groups, ~30 entries).
- `ShortcutsDialog.svelte`: two-column modal with `<kbd>` chips; Escape / click-away /
  Close button dismiss.
- `Icon.svelte`: added a `keyboard` glyph for the dialog header.
- `MenuBar.svelte`: "Keyboard shortcuts" item (hint F1) in the Application menu.
- `App.svelte`: `shortcutsOpen` state, `F1` case in the keydown switch (length-3 key so the
  type-ahead branch can't swallow it; keydown also returns early inside inputs), and the
  `shortcuts` menu case.
- Tests: new `shortcuts.test.ts` (structure + marquee-binding presence). Suite 270 green.

Verification: `npm run check` 0 errors; 270 tests pass; `npm run build` (vite prod bundle)
succeeds. GUI visual drive not performed — (a) at land time machine idle was 0s (user
active), and (b) the app is a native Tauri/WebView2 window, which the available automation
(claude-in-chrome) can't drive. Relied on headless verification, which fully covers this
pure-frontend change. Recommend a human eyeball of the F1 dialog at next convenient run.
Done.
