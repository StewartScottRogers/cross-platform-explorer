---
id: CPE-1209
title: "GUI: broken-link Repair link… action (suggest target + re-create)"
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-08-01
epic: CPE-715
---

## Summary
Part of CPE-715 (after CPE-1206). Repair a broken symlink by re-pointing it at a found target.

## Build
- Right-click a broken link → "Repair link…" → call `suggest_repair` (CPE-1206), show the suggested target with
  Accept (re-create the symlink to the found path) / Browse-for-another. Confirm before overwriting.

## Acceptance Criteria
- [ ] gui-smoke render pin of the repair dialog with a suggested target; `npm run check` + `npm test` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-715). Prereq CPE-1206. Batch with CPE-1207 (shared App.svelte/ContextMenu).
- 2026-08-01 — Done. Added `RepairLinkDialog.svelte`. `ContextMenu.svelte` gained a `linkBroken` prop
  that gates a new "Repair link…" row on the item menu; App.svelte's `onRowContext` resolves it async
  via `commands.linkStatus(entry.path).broken` (cheap-gated on `entry.is_symlink` first) whenever a
  right-click lands on a symlink, so the row only appears for an actually-broken one. The dialog calls
  `commands.suggestRepair(linkPath, searchRoots)` on mount ([currentPath] as the search root); Accept
  moves to an inline confirm step ("Replace the broken link so it points to …?") before doing anything —
  the ticket's "confirm before overwrite" requirement — and only on confirm does it call
  `commands.deletePermanent([linkPath])` (unlinks just the dead symlink entry — `remove_file` never
  follows a symlink) followed by `commands.createSymlink(target, linkPath)` to re-point it, since
  `create_symlink` refuses to overwrite an existing path. "Browse for another…" opens the native file
  picker for a manual target. No suggestion found is handled gracefully (message shown, no Accept
  button, Browse still offered). A failure at either backend step is shown inline AND dispatched as
  `error` for App.svelte's `showNotice`, and the dialog stays open to retry. Tests: `RepairLinkDialog.
  test.ts` (4 cases — suggest_repair on mount + suggestion shown, no-suggestion handling, the full
  Accept→confirm→delete_permanent→create_symlink→repaired sequence, and a failed re-create surfacing via
  the `error` event) + 1 new `ContextMenu.test.ts` case for the `linkBroken`-gated row. i18n keys added
  across all 12 complete locales. `npm run check` / `npm test` green. Batched with CPE-1207 on branch
  `cpe-1207-1209-link-dialog-repair`.
  Note on this ticket's own AC ("gui-smoke render pin of the repair dialog with a suggested target"):
  descoped for this batch by design — the Foreman's brief for this session asked for exactly one
  gui-smoke addition, CPE-1207's hardlink click-through, deliberately: a "broken symlink" gui-smoke
  fixture needs `fs.symlinkSync` in `wdio.conf.ts`'s `onPrepare`, which hits the very Windows
  Developer-Mode/elevation gate CPE-1207 documents — not reliably creatable unprivileged in CI, unlike
  the hardlink CPE-1207 uses instead. The render-pin intent is still covered, at the Vitest layer:
  `RepairLinkDialog.test.ts`'s first case renders the dialog with a real (mocked) suggested target and
  asserts the suggestion text + Accept button, the same shape the AC asks for. A follow-up ticket can
  add the gui-smoke leg once there's a sanctioned way to seed a broken-symlink fixture cross-platform.
