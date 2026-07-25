---
id: CPE-637
title: Assign tags + colour label via the context menu
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
estimate: 2h
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

# CPE-637 — Assign tags + colour label via the context menu

## Summary

Give the user a way to attach tags and a single colour label to a file or folder, built on the
CPE-636 frontend tag service. A new **Tags…** item in the file/folder right-click context menu opens
a small editor popover: current tags as removable chips, an input that adds a tag on Enter, and a
colour-label swatch row. Applying persists via `setEntryTags`. Frontend only — no `src-tauri/`
changes.

## Acceptance Criteria

- [x] A **Tags…** item is added to the item (file/folder) context menu, shown for a single selection.
- [x] The menu item follows `docs/design/MENUS.md` — text is `var(--text)`, a leading `<Icon>` at
      `size={15}`, i18n label via `$t('ctx.tags')`.
- [x] A `TagEditor.svelte` popover shows the entry's current tags as removable chips.
- [x] An input adds a tag on Enter (trimmed, de-duplicated, empty ignored); Backspace on an empty
      input peels the last chip.
- [x] A colour-label picker offers the `LABEL_COLORS` swatches (none + red/orange/yellow/green/blue/
      purple/grey), single-select, with the current label pre-selected.
- [x] Apply calls `setEntryTags(path, tags, label)`; Cancel / Esc / click-outside close without saving.
- [x] The dialog has a visible border (`--border-strong`) and uses theme variables throughout.
- [x] `initTags()` is called once in `App.svelte`'s `onMount` (next to `initTransfers()`).
- [x] `npm run check` clean; full `npx vitest run` green.

## Resolution

- Added `ctx.tags` to the context menu (`ContextMenu.svelte`), single-selection only, wired to a new
  `"tags"` action in `App.svelte`'s `runAction` that opens the editor for the selected entry.
- Added a `tag` glyph to `Icon.svelte`.
- Built `src/lib/components/TagEditor.svelte`: seeds a working copy from the store via
  `entryFor(get(tags), path)`, renders removable tag chips (reflow container, nowrap chips), an add
  input (Enter adds, Backspace peels), and a single-select colour swatch row from `LABEL_COLORS`.
  Apply persists via `setEntryTags` then closes; Cancel/Esc/backdrop close without saving.
- Rendered `<TagEditor>` from `App.svelte` behind `tagEditorFor`, and call `initTags()` in `onMount`.
- Added the `ctx.tags` + `tags.*` i18n keys (title, remove, none, addLabel, addPlaceholder,
  colorLabel, cancel, apply, and the 8 `tags.color.*` names) to all 12 complete locales so the
  CPE-481/539 coverage gate stays green.

## Work Log

- Studied `src/lib/tags.ts`, `ContextMenu.svelte`, `App.svelte` (context-menu dispatch + onMount),
  `PatternSelectDialog.svelte` (dialog convention), and `docs/design/MENUS.md`.
- Implemented the menu item, action, icon, editor component, i18n keys, and App wiring.
- Verified: `npm run check` → 0 errors / 0 warnings; `npx vitest run` → 69 files / 652 tests green.
