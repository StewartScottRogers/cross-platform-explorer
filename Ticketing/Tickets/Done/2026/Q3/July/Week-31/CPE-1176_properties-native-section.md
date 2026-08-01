---
id: CPE-1176
title: "Surface native metadata (store name, tags, comment) in PropertiesDialog"
type: feature
component: Frontend
priority: medium
status: Doing
tags: ready
created: 2026-07-31
epic: CPE-717
---

## Summary
Part of the CPE-717 GUI remainder. Add a read-only "Native metadata" section to `PropertiesDialog`, shown only
when `nativeBridgeEnabled` (the key owned by CPE-1177). Displays the store display name (`nativeTagsName`
command), the file's native tags/comment (`native_tags_pull`), and a Pull/refresh button. Built with CPE-1177
by the same worker on one branch (it consumes CPE-1177's setting key).

## Build
- In `src/lib/components/PropertiesDialog.svelte`, add a "Native metadata" section gated on `nativeBridgeEnabled`:
  calls the existing `native_tags_pull` / `nativeTagsName` commands (see `bindings.gen.ts`), renders tags +
  comment read-only, with a Pull button to (re)load. Reuse the app's dialog/section styling + visible border
  ([[dialogs-need-visible-border]]); pills must reflow ([[tick-tacks-reflow]]).
- No write path here (that's TagEditor's job) — read-only surfacing.

## Acceptance Criteria
- [ ] `PropertiesDialog.test.ts` (jsdom): the native section renders when `nativeBridgeEnabled` is on and is
      hidden when off.
- [ ] gui-smoke screenshot shows the native section populated for a tagged file (pinned in CPE-1178).
- [ ] `npm run check` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-717). Consumes CPE-1177's `nativeBridgeEnabled`; same worker.
