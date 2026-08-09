---
id: CPE-1176
title: "Surface native metadata (store name, tags, comment) in PropertiesDialog"
type: feature
component: Frontend
priority: medium
status: Done
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
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-717). Consumes CPE-1177's `nativeBridgeEnabled`; same worker.
- 2026-07-31 — Done. Added a bordered, read-only "Native metadata" section to `PropertiesDialog.svelte`,
  gated on `nativeBridgeEnabled` (CPE-1177) and shown only for a single selected entry. Calls
  `nativeTagStoreName()` (`native_tags_name`) for the store display name and a Pull button calls
  `pullNativeTags(path)` (`native_tags_pull`) to re-seed the tags shown, which are read reactively from
  CPE's own tag store (`entryFor($tags, path)`) — no write path here (TagEditor keeps that job). Note: the
  shipped native-metadata model is `{tags, label}` only (`crates/server/src/native_bridge.rs` /
  `native_tags.rs`) — there is no separate "comment" field in the backend, so the section surfaces the
  colour **Label** in that slot rather than inventing a new field. Tags render as reflowing pills
  (flex-wrap container, nowrap chips) and the section has its own visible border, matching the dialog
  conventions. Tests: `PropertiesDialog.test.ts` — hidden when the flag is off, and when on renders the
  store name + tags + label after Pull. `npm run check` 0 errors; `npm test` all green. Built together with
  CPE-1177 on branch `cpe-1177-1176-native-metadata-gui`, PR opened against `main`.
