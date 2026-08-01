---
id: CPE-1212
title: "App-wide: centralize hard-coded error/danger colours into a --danger theme var"
type: chore
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-08-01
epic: CPE-579
---

## Summary
Recurring reviewer nit across epics (CPE-1189 MacrosDialog `.err` #c0392b/#d05656; CPE-1208 LinkBadge
`.broken` #b5433a; plus pre-existing `FileList.svelte .agent-badge.removed`, `ExplorerPane.svelte`,
`SidecarManager.svelte`, `AgentTimeline.svelte`). Error/danger colours are hard-coded hex literals scattered
across components instead of a single theme variable. Not a MENUS violation (those target popup-menu item
text, which is correctly `var(--text)`), and the app is light-theme-only — so purely a consistency/maintainability
cleanup.

## Build
- Add a `--danger` (and maybe `--warn`) token to the single `:root` palette ([[app-is-light-theme-only]]).
- Replace the scattered hard-coded error/danger hexes with `var(--danger)` across the components above.
- No visual change intended (pick the token value to match the current predominant hex).

## Acceptance Criteria
- [x] Error/danger colours come from `var(--danger)`; grep shows no stray `#c0392b`/`#b5433a`/`#d05656` in
      component styles; `npm run check` + `npm test` green; no visual regression (Visual Critic spot-check).

## Work Log
- 2026-08-01 — Filed by Foreman (workshift) from repeated reviewer nits (CPE-1189, CPE-1208). App-wide polish,
  like the dialog-border CPE-1193.
- 2026-08-01 — Worker: implemented, mirroring the CPE-1193 codemod pattern. Added `--danger: #c42b1c` (plus
  `--danger-hover: #a82419`, needed for the two dialogs that had their own darker hover shade) to the single
  `:root` in `src/app.css`. `#c42b1c` was chosen as the canonical value: grepped all danger/error reds across
  `src/**/*.svelte` + `src/app.css` and found four distinct hexes in use — `#c42b1c` (37 occurrences / 18
  files, already the base `.error` colour), `#c0392b` (27 / 14), `#d05656` (5 / 5), `#b5433a` (9 / 7) — plus
  two more one-off strays found only by grepping existing `var(--danger, ...)` fallback usages that had never
  actually been backed by a real token: `#c9372c` (PropertiesDialog) and `#d33` (MetadataStudioDialog). All
  six were unified into `--danger`; the several existing `var(--danger, <hex>)` call sites had their now-dead
  fallback stripped to a plain `var(--danger)`.
  Swapped 39 files total (38 components + `app.css`'s own `.error`): AboutDialog, AgentTimeline (5 spots,
  incl. the `.cp-btn.danger`/`.cp-confirm` block whose comment explicitly said "the palette has no dedicated
  destructive token" — now it does), AttributesDialog, BackupDashboard (3), BatchMediaDialog (also fixed a
  mismatched-var-name bug: `.status .warn` and `.skip-reason` were reading `var(--warn, #c42b1c)` — a *red*
  fallback under the amber `--warn` name — corrected to `var(--danger)`), BatchRenameDialog, CardDetailDialog,
  CheckpointDialog (4), CompareDialog (5, the diff "removed" family), ConfirmDialog (2, incl. new
  `--danger-hover`), ConflictDialog, ContentSearchDialog, DiffPeek, DiffSideBySide, DuplicatesDialog (2),
  ExplorerPane, FileList, FileNameSearchDialog, InstantSearch, IntegrityDialog (4), LinkBadge (CPE-1208's
  `.broken` badge), MacroRunConfirm, MacrosDialog (2), MetadataStudioDialog, NearDuplicatesDialog,
  OrganizeDialog, PreviewPane, PropertiesDialog (4), RunCommandConfirm, ScheduledSnapshots,
  SessionHistoryDialog, ShellIntegration, Sidebar (`.drive-bar-fill.full`), SidecarManager, SimilarImagesDialog
  (2), SpotlightHotkeySettings, TemplatesDialog (2), TransferPanel, UpdateDialog.
  Deliberately left untouched: `Icon.svelte`'s `#c42b1c` (6 occurrences) — that's the fixed illustrative
  stroke/fill for the "pdf" file-type glyph (Office-style per-filetype icon colours: yellow folder, green
  image, blue document, red pdf, etc.), a coincidental hex match with no error/danger semantics, not part of
  this ticket's scope. Confirmed zero menu components touched (no ContextMenu/Submenu/TabMenu/TagMenu/
  SmartFolderMenu/AgentMenu/MenuBar in the diff) — MENUS.md's "item text is always `var(--text)`" rule is
  untouched, no red menu text introduced or removed.
  `npm run check`: 0 errors/0 warnings. `npm test`: 148 files / 1645 tests passed — no test asserted an old
  danger hex, so nothing needed updating. Final grep for `#c42b1c|#c0392b|#d05656|#b5433a|#c9372c|#d33` across
  `src/` returns only the `app.css` token definition + its doc comment, the intentionally-untouched
  `Icon.svelte`, and an unrelated pre-existing test (`ai-console-launcher.test.ts`, asserting absence of
  `#d05656` in a different launcher HTML template, not part of this component sweep). GUI re-verification
  left to the Foreman's Visual Critic pass (no visual change intended — same predominant hex, different
  spellings unified).
