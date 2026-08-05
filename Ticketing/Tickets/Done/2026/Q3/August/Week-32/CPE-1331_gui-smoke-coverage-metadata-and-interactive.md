---
id: CPE-1331
title: "gui-smoke coverage: new metadata-studio spec + interactive-state snaps (exclude pill, enabled Move-to-Bin)"
type: test
component: frontend
priority: medium
status: Done
tags: ready
created: 2026-08-05
epic: CPE-1148
---

## Summary
Closes two QA-coverage gaps flagged during the 2026-08-05 GUI sweep (recorded in
`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`):
1. **Metadata Studio has NO gui-smoke render coverage** — four tickets (CPE-1325/1326/1327/1328) shipped
   against `MetadataStudioDialog.svelte` with only jsdom logic tests; the real render/visual is still manual.
2. **File-Health / near-dup specs capture only the RESTING state** — they never type an exclude pattern (so no
   filled exclude-pill row is ever screenshotted) or check a cleanup box (so the enabled "Move N to Recycle Bin"
   is never screenshotted). The Visual Critic can only judge the empty states.

## Build
- **New `gui-smoke/specs/metadata-studio.smoke.ts` + a `seedMetadataStudioFixture` in `gui-smoke/wdio.conf.ts`:**
  seed a small WRITABLE media file (e.g. a tiny valid mp3 or jpg — reuse an existing fixture asset if one exists;
  check what other specs seed) into the fixture dir, drive the real `tauri build` binary to open the Metadata
  Studio on it (match how the dialog is opened — likely selecting the media file then a menu/palette action),
  and assert the editable metadata fields render. `snap()` the dialog. Follow the exact pattern of
  `gui-smoke/specs/file-health.smoke.ts` + `seedFileHealthFixture`.
- **Extend `gui-smoke/specs/file-health.smoke.ts`:** after opening File-Health, type an exclude pattern into the
  exclude input (or click a quick-add chip), then `snap()` so a FILLED exclude-pill row is captured.
- **Extend `gui-smoke/specs/near-duplicates.smoke.ts`:** after the scan renders findings, check one item's box,
  then `snap()` so the ENABLED "Move N to Recycle Bin" state is captured. Do NOT actually click Move-to-Bin
  (don't mutate fixtures / trigger a real trash).

## Acceptance criteria
- `npx wdio run ./wdio.conf.ts --spec ./specs/metadata-studio.smoke.ts` PASSES against a real `tauri build`
  binary and asserts the Studio's editable fields render.
- The File-Health and near-dup specs still pass AND now capture a filled-exclude-pill and an enabled-Move-to-Bin
  screenshot respectively.
- `npx tsc --noEmit -p tsconfig.json` (gui-smoke/) clean.

## Notes
- **You MUST actually run the wdio build, not just type-check it** — the CPE-1330 escaped defect happened
  precisely because a spec was only type-checked. Report the real pass output.
- Non-blocking CI (gui-smoke is `continue-on-error` per CPE-1048); this raises real render coverage + gives the
  Visual Critic the interactive states it was missing. Retires/advances burndown rows for the metadata + GUI batch.
- Conflict surface: `gui-smoke/specs/metadata-studio.smoke.ts` (new), `file-health.smoke.ts`,
  `near-duplicates.smoke.ts`, `wdio.conf.ts`. Reference: `file-health.smoke.ts` + `seedFileHealthFixture`.
