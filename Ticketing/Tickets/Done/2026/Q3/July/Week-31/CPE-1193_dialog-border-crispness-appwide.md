---
id: CPE-1193
title: "App-wide: make modal dialog borders read as a clearly-visible edge, not just a shadow"
type: chore
component: Frontend
priority: low
status: Doing
tags: ready
created: 2026-07-31
epic: CPE-579
---

## Summary
Visual-Critic observation during the CPE-705 pass: the PasswordPromptDialog reads as resting on a drop-shadow
halo with no crisp edge. On inspection its border CSS is **identical to `ConfirmDialog.svelte` and the shared
dialog standard** (`.dialog { border: 1px solid var(--border-strong); box-shadow: 0 20px 50px rgba(0,0,0,.25) }`)
— so this is **app-wide**, not specific to one dialog. Per [[dialogs-need-visible-border]] every dialog should
have a *clearly-visible* thin border, not just a shadow; the current `--border-strong` on a white surface over a
dimmed backdrop can read as shadow-only, especially on smaller modals.

## Build
- Raise dialog-edge crispness **consistently across all modals** — e.g. bump the shared `.dialog` border to a
  more contrasting token (or add a subtle inset ring) in the common dialog styling, so ConfirmDialog,
  PasswordPromptDialog, PropertiesDialog, SettingsDialog, TemplatesDialog, etc. all gain the crisper edge
  together. Do NOT fix a single dialog in isolation (that breaks the design-system consistency).
- Re-verify a few dialogs via gui-smoke screenshots + the Visual Critic afterwards (light-theme only).

## Acceptance Criteria
- [x] Modal dialogs show a clearly-visible thin border (not shadow-only) uniformly; no single dialog is an
      outlier; `npm run check` + tests green.

## Work Log
- 2026-07-31 — Filed by Foreman (sprint) from the CPE-705 Visual-Critic finding; scoped app-wide because the
  password modal already matches the shared dialog border standard exactly.

## Evidence 2026-08-01 (sprint)
The epic-704/CPE-1221 Visual Critic pixel-sampled the `NearDuplicatesDialog` edges and found the
`1px solid var(--border-strong)` present in CSS does NOT render as a distinct border line against the
dimmed backdrop — it reads only as a white→grey transition (border colour ≈ backdrop value). Concrete
confirmation that `--border-strong` is too weak for the dialog-on-dimmed-backdrop case this ticket
targets. Whatever fix lands here should be verified by re-capturing a dialog screenshot and pixel-
sampling the edge, not just by reading the CSS. Applies to all dialogs sharing this pattern
(SimilarImagesDialog, NearDuplicatesDialog, DuplicatesDialog, …).

## Work Log (fix)
- 2026-08-01 — Worker: centralized fix. Introduced a dedicated `--dialog-border: #3c3c3c` token in the
  single `:root` palette (`src/app.css`), separate from `--border-strong` (#b3b3b3, left unchanged —
  it's still consumed by ~19 non-dialog chrome selectors: address bar, menus, pills, iconbtn, settings
  row/btn — changing it globally would have regressed all of those). `--dialog-border` is picked to stay
  >=3:1 contrast against both the white dialog surface and the darkest backdrop in use
  (`rgba(0,0,0,.45)` over `--bg #f3f3f3` ≈ `#868686`), where the old `--border-strong` (#b3b3b3) was
  nearly identical to the lightest backdrop (`rgba(0,0,0,.25)` ≈ `#b6b6b6`) — the exact blend the
  Visual Critic pixel-sampled.
  Swapped `var(--border-strong)` → `var(--dialog-border)` on the outer modal-panel border only (not
  internal buttons/inputs, which correctly keep `--border-strong`) in 50 components via a scoped codemod
  (regex-matched the specific `.dialog{}`/panel selector block per file, verified 1 replacement per file):
  43 sharing the literal `.dialog {}` selector (AboutDialog, AttributesDialog, BackupDashboard,
  BatchMediaDialog, BatchRenameDialog, CardDetailDialog, CheckpointDialog, ColorRulesDialog,
  ColumnPickerDialog, CompareDialog, ConfirmDialog, ConflictDialog, ContentSearchDialog, DiffSideBySide,
  DiskSpaceView, DuplicatesDialog, FileNameSearchDialog, IntegrityDialog, MacroParamPrompt,
  MacroRunConfirm, MacrosDialog, MetadataStudioDialog, NearDuplicatesDialog, NewLinkDialog,
  OrganizeDialog, PasswordPromptDialog, PatternSelectDialog, PropertiesDialog, RepairLinkDialog,
  RunCommandConfirm, SelectByDialog, SessionHistoryDialog, SettingsDialog, ShortcutsDialog,
  SimilarImagesDialog, SyncDialog, TagEditor, TemplatesDialog, TransferConflictDialog, UpdateDialog,
  UserCommandsDialog, WatchRulesDialog, WorkspacesDialog) plus 7 modal overlays using a bespoke panel
  class confirmed to be dimmed-backdrop modals (BoardView `.board-panel`, CommandPalette `.cp-panel`,
  DocsView `.docs-panel`, InstantSearch `.is-panel`, RepoBrowser `.repo-panel`, Spotlight `.sp-panel`,
  WorkbenchView `.wb-panel`). Non-modal menus/popovers/toasts that also use `--border-strong`
  (ContextMenu, Submenu, TabMenu, TagMenu, SmartFolderMenu, AgentMenu, MenuBar, Toolbar, StatusBar,
  ContextBar, TransferPanel, FloatPreview, PreviewPane, DataBrowser, ScheduledSnapshots,
  SpotlightHotkeySettings, AgentTimeline, App.svelte) were deliberately left untouched — verified each is
  a cursor/button-anchored flyout or docked toolbar chrome, not a centered dimmed-backdrop dialog.
  `npm run check`: 0 errors/warnings. `npm test`: 147 files / 1629 tests passed. No test asserted the old
  border CSS, so nothing needed updating. GUI re-capture + pixel-sample of the edge left to the Foreman's
  Visual Critic pass (needs a release build).
