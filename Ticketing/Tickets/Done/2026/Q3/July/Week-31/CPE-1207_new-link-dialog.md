---
id: CPE-1207
title: "GUI: New Link… dialog + creation wiring (symlink/hardlink)"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-08-01
epic: CPE-715
---

## Summary
Part of CPE-715 (after CPE-1206). A "New Link…" dialog to create symlinks/hardlinks.

## Build
- "New Link…" entry in the empty-area context menu + command palette; a small dialog: kind (Symlink | Hardlink),
  target field with a native Browse picker ([[path-inputs-need-picker]]), link-name field. On confirm call
  `commands.createSymlink`/`createHardLink`, reload + inline-rename like `createNewItem` (`App.svelte`).
- Surface the backend's Windows Developer-Mode/elevation error via `showNotice` (do NOT swallow it; no elevation
  modal — [[avoid-modal-permission-popups]]).

## Acceptance Criteria
- [x] gui-smoke: render pin of the dialog; a headless click-through creating a **hardlink** (unprivileged-safe)
      in a temp dir that then lists. `npm run check` + `npm test` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-715). Batch with CPE-1209 (both edit App.svelte/ContextMenu).
- 2026-08-01 — Done. Added `NewLinkDialog.svelte` (kind Symlink|Hardlink, target field + native Browse
  picker via `@tauri-apps/plugin-dialog`, link-name field). "New Link…" wired into the empty-area New ▸
  submenu (`ContextMenu.svelte`) and the command palette (`file.newLink`); on confirm the dialog calls
  `commands.createSymlink`/`createHardLink` itself, and App.svelte's `onNewLinkCreated` reloads +
  inline-renames the new link via the same `pendingRenamePath` hook `createNewItem` uses. A backend
  failure — including the Windows Developer-Mode/elevation error `create_symlink` can return — is shown
  inline in the dialog AND dispatched as `error` for App.svelte to surface via the app-wide `showNotice`
  toast; the dialog never auto-closes on failure and there is no elevation modal
  ([[avoid-modal-permission-popups]]). Added a `link` glyph to `Icon.svelte` and i18n keys across all 12
  complete locales (en/es/de/fr/it/pt/nl/pl/ru/zh/ja/ko). Tests: `NewLinkDialog.test.ts` (4 cases —
  correct `create_symlink`/`create_hard_link` call, empty-field local validation, Windows-error-style
  rejection surfaced via the `error` event) + 1 new `ContextMenu.test.ts` case for the New ▸ New Link…
  row. gui-smoke: `new-link.smoke.ts` render-pins the dialog and drives a headless click-through that
  creates a HARDLINK (unprivileged-safe, cross-platform) in a dedicated seeded empty folder, then
  asserts it lists and its on-disk content matches the link target — written and `gui-smoke && npm run
  typecheck`-clean here; the LIVE run (real built app + tauri-driver) is CI, not this sandbox, per the
  standing gui-smoke convention. `npm run check` / `npm test` green. Batched with CPE-1209 on branch
  `cpe-1207-1209-link-dialog-repair`.
