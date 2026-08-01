---
id: CPE-1178
title: "gui-smoke pin + docs page for the native-metadata bridge"
type: chore
component: Testing
priority: low
status: Done
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
- [x] `gui-smoke/specs/native-tags.smoke.ts` exists and asserts the native section renders (CI-exercised,
      non-blocking per CPE-1048); `cd gui-smoke && npm run typecheck` green.
- [x] Docs page added + `sectionDocs.ts` entry; `src/lib/sectionDocs.test.ts` guard passes; `npm run check` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-717). Wave 2 (after 1176/1177). Disjoint files.
- 2026-07-31 — Done by Worker. Added `gui-smoke/specs/native-tags.smoke.ts`: opens Settings via the
  Command Palette, flips `native-bridge-toggle` on (asserting it's off by default first), closes
  Settings, selects the seeded marker file, opens Properties via `Alt+Enter`, and asserts
  `[data-testid="native-metadata-section"]` + its `.chips` container render; `snapFailure`/`snap`
  wired per CPE-1149. Added `src/docs/17-native-metadata.md` (bridge, toggle, Properties section,
  Pull/Push, Native Tags column, platform notes) and registered a new `"native-metadata"` entry in
  `src/lib/sectionDocs.ts` → `17-native-metadata`. `npm run check` (0 errors), `npm test` (131 files /
  1489 tests incl. `sectionDocs.test.ts`), `gui-smoke` `npm ci` + `npm run typecheck` all green. The
  live gui-smoke browser run itself was not exercised locally (needs a real `tauri build` +
  msedgedriver) — that's CI's job, non-blocking per CPE-1048.
