---
id: CPE-1317
title: File-Health panel — slice 3 (empty-dirs tab, non-streaming) + already-open tab-switch fix
type: feature
component: Frontend
priority: medium
tags: ready
created: 2026-08-04
epic: CPE-1002
estimate: 2h
---

## Summary
Slice 3 of the File-Health panel. Adds the **empty-directories** tab (collect-to-vec, NOT streaming —
`find_empty_dirs` has no `_stream` variant) and fixes a UX papercut found in slice 2's UAT.

## Backend command
- `find_empty_dirs(root, excludes) → Result<EmptyDirsReport, String>`; `EmptyDirsReport { dirs: string[],
  scanned, truncated }`. NON-streaming — a plain awaited call. Use `invoke` from `src/lib/invoke.ts`
  (busy-cursor), NOT rawInvoke, since there's no Channel. Model the tab body on how `NearDuplicatesDialog.svelte`
  does its plain awaited scan (not the streaming tabs). Rows = each empty dir's name + parent location.

## The tab-switch fix (from slice 2 UAT)
`FileHealthDialog` takes `activeTab = initialTab` as a ONE-TIME initializer; because `{#if fileHealthOpen}`
doesn't remount, invoking a *different* File-Health tool entry while the panel is ALREADY OPEN doesn't jump to
that tab. Fix so that opening/re-invoking a specific tool entry switches the panel to that tab even when
already open (e.g. App.svelte bumps an open-nonce alongside `fileHealthTab`, and the dialog reacts to it to set
`activeTab` — pick the clean Svelte-idiomatic approach; must NOT break manual in-panel tab clicks, and must
handle re-invoking the SAME entry while manually on another tab).

## Acceptance Criteria
- [ ] Empty-dirs tab added to the panel (4th tab) with a plain awaited `find_empty_dirs` call, loading/empty/
      error states, rows reveal→navigate+close, footer scanned/capped. Pick an EXISTING Icon glyph.
- [ ] Tools-menu + Command-Palette entry `find-empty-dirs` opening the panel to that tab (append, don't reflow).
- [ ] Already-open panel switches to the requested tab when a tool entry is invoked (fix verified by a jsdom
      test: open on dangling, invoke empty-dirs entry → panel shows empty-dirs tab).
- [ ] jsdom tests for the empty-dirs tab (loading, empty, error, navigate+close, rescan-replaces) + the
      tab-switch fix. `npm run check` clean + full `npm run test:unit` green (i18n 12-locale + sectionDocs guards).
- [ ] i18n new keys × 12 locales; extend `src/docs/22-file-health.md`.

## Work Log
2026-08-04 (workshift run 2) — Filed by the Foreman. Slice 3 of 4 (archive-safety is slice 4, separate).
