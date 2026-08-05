---
id: CPE-1330
title: "Fix declutter.smoke.ts: assert reason label on the group header, not the row"
type: bug
component: frontend
priority: high
status: Done
tags: ready
created: 2026-08-05
epic: CPE-979
---

## Summary
The `gui-smoke/specs/declutter.smoke.ts` spec (shipped with CPE-1329) fails on a real `tauri build` — but the
DIALOG is correct; the SPEC's assertion is wrong. `findRowContaining` locates a `[data-testid="dc-row"]`
element and asserts the reason text (e.g. "Empty file") is inside that row's HTML. But `DeclutterDialog.svelte`
renders the reason label once on the GROUP HEADER (`[data-testid="dc-group"]`, `{reasonLabel(g.reason)}
({g.rows.length})`) as a sibling of the rows — each `dc-row` contains only a checkbox + filename button. So the
spec checks the wrong element and reports a false negative. gui-smoke is non-blocking (CPE-1048) so it didn't
break CI hard, but a red spec on main is a real escaped defect (it slipped because the ~4min build wasn't run at
build/review time — only type-checked).

## Build
- Fix the assertions in `gui-smoke/specs/declutter.smoke.ts` to verify each reason label on its
  `[data-testid="dc-group"]` header (with its count, e.g. "Empty file (1)"), and separately verify the seeded
  filename renders in a `dc-row` under that group. Keep asserting all four seeded findings
  (`declutter-empty.log` / `declutter-setup.exe` / `declutter-movie.mp4.part` / `declutter-notes.txt.bak`) render
  under their correct four reason groups (Empty file / Installer / Temporary-partial / Backup-leftover).
- Confirm the "Move 0 to Recycle Bin" disabled-at-zero-selection assertion still holds (it was correct).

## Acceptance criteria
- `npx wdio run ./wdio.conf.ts --spec ./specs/declutter.smoke.ts` from `gui-smoke/` **PASSES** against a real
  `tauri build` binary (the fixer MUST run it, not just type-check — this bug escaped precisely because the spec
  was only type-checked).
- No change to `DeclutterDialog.svelte` (the dialog is correct) unless a genuinely-missing `data-testid` is
  needed for a clean assertion — if so, keep it minimal.

## Notes
- FRONTEND/test-only. This is the escaped-defect follow-up to CPE-1329; the dialog itself was Visual-Critic-
  verified. Source: GUI-verifier run 2026-08-05.
