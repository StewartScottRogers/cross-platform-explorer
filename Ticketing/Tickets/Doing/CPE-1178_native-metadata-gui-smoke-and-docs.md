---
id: CPE-1178
title: "gui-smoke pin + docs page for the native-metadata bridge"
type: chore
component: Testing
priority: low
status: Doing
tags: ready
created: 2026-07-31
epic: CPE-717
---

## Summary
Part of the CPE-717 GUI remainder. Pin the native-metadata GUI in gui-smoke and document the feature. Built in
**wave 2** — after CPE-1176/1177 land — so the spec drives the real merged UI (toggle + Properties native
section) and uses the actual selectors.

## Build
- New `gui-smoke/specs/native-tags.smoke.ts` driving the real built app: enable the `nativeBridgeEnabled` toggle
  in SettingsDialog, open Properties for a seeded file, assert the "Native metadata" section renders, `snap`
  a screenshot; add the `afterEach` `snapFailure` per the CPE-1149 convention.
- Add/extend a docs page in `src/docs/*.md` for the native-metadata bridge and register its `section → slug`
  entry in `src/lib/sectionDocs.ts` (the `sectionDocs.test.ts` guard requires it) per [[maintain-in-app-docs-library]].

## Acceptance Criteria
- [ ] `gui-smoke/specs/native-tags.smoke.ts` exists and asserts the native section renders (CI-exercised,
      non-blocking per CPE-1048); `cd gui-smoke && npm run typecheck` green.
- [ ] Docs page added + `sectionDocs.ts` entry; `src/lib/sectionDocs.test.ts` guard passes; `npm run check` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-717). Wave 2 (after 1176/1177). Disjoint files.
