---
id: CPE-1167
title: "gui-smoke: pin the ColumnPickerDialog render (retire its manual-verify debt)"
type: chore
component: Testing
priority: medium
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-579
---

## Summary
QA-Architect / PM-scouted (2026-07-31). The **ColumnPickerDialog** (epic CPE-707: CPE-1146/1147, shipped
2026-07-30) is the newest GUI surface and has **no `gui-smoke` render pin** — its look is unverified by the
automated harness. Add one so the picker is caught by CI like the other pinned surfaces.

## Build
- New `gui-smoke/specs/column-picker.smoke.ts` (model on the existing specs + `lib/snap.ts` from CPE-1148):
  seed a folder with a few files, open the ColumnPickerDialog (via its real entry point — command palette
  "columns" / the toolbar/`ExplorerPane` control), assert the available-columns list + the grid layout render,
  and `snap("column-picker")` for the Visual Critic.
- If **CPE-1166 has merged** by the time this runs, also assert the new True-type / Text-encoding columns
  appear in the list (nice pairing). If 1166 hasn't merged yet, just pin the existing columns — don't block.
- Keep it non-blocking (`continue-on-error`, CPE-1048) like the rest of gui-smoke. Capture-on-failure
  (CPE-1149 `snapFailure`) applies.

## Acceptance Criteria
- [ ] `column-picker.smoke.ts` opens the picker on the real built app and asserts its available-columns list
      renders; a real run produces `column-picker.png` in `.screenshots/`.
- [ ] `npm run check` green; `gui-smoke` typecheck clean; the spec passes against a fresh binary (this machine
      has tauri-driver + msedgedriver — build the binary + run it).
- [ ] Flips the picker's manual-verify status to automated; note the pinning spec.

## Notes
- Test-infra only; no app-code change. Epic CPE-579. Pairs naturally with CPE-1166.
