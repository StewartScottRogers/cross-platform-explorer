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
- [x] `column-picker.smoke.ts` opens the picker on the real built app and asserts its available-columns list
      renders; a real run produces `column-picker.png` in `.screenshots/`.
- [x] `npm run check` green; `gui-smoke` typecheck clean; the spec passes against a fresh binary (this machine
      has tauri-driver + msedgedriver — build the binary + run it).
- [x] Flips the picker's manual-verify status to automated; note the pinning spec.

## Notes
- Test-infra only; no app-code change. Epic CPE-579. Pairs naturally with CPE-1166.

## Work Log
- 2026-07-31 — Added `gui-smoke/specs/column-picker.smoke.ts`, the render pin for `ColumnPickerDialog.svelte`
  (CPE-1146, epic CPE-707). Modelled on `organize.smoke.ts`: reuses the existing `--open=<tmpDir>` fixture
  (the seeded tmpDir already carries several files, so `inFolder` is true and no new fixture was needed),
  opens the picker through its REAL entry point — the Command Palette (Ctrl+Shift+P → type "columns" →
  click the "Manage columns…" row, the `tool.columns` command in `App.svelte`), and asserts:
  - the dialog (`.dialog[role="dialog"]`) mounts;
  - the available-columns list (`[data-testid="available-list"]`) renders and is **non-empty** (a falsifiable
    guard — an empty list would mean the `metadata_columns_available` catalog fetch failed);
  - **all four CPE-1166 magic-byte detector columns are present** — asserted both by exact add-button testid
    (`add-detect.true_type`, `add-detect.type_mismatch`, `add-detect.text_encoding`, `add-detect.line_endings`,
    keyed to `MetaColumn::id()`) AND by their display label ("True Type" / "Type Mismatch" / "Text Encoding" /
    "Line Endings", from `MetaColumn::label()` in `crates/server/src/column_extract.rs`) appearing in the list
    HTML. This makes the spec the end-to-end confirmation that CPE-1166's columns surface in the real app.
  - `snap("column-picker")` captures the good frame for the Visual Critic (CPE-1148); `snapFailure` in
    `afterEach` (CPE-1149) captures a `-fail.png` on any failure. Non-destructive: never adds/removes a
    column, closes via the Close button (`done-btn`).
- Harness run (this machine — tauri-driver + msedgedriver): built the frontend + a fresh release binary from
  branch HEAD (based on main `173b82e2`, i.e. CPE-1166 merged) via `npm run build && npm run tauri build --
  --no-bundle`, then ran the single spec. Result: **1 passing (4.2s)** — the picker opened and all four
  detector columns were found in the available list. `column-picker.png` (78 KB) landed in
  `gui-smoke/.screenshots/`; opened it — shows the "Manage columns" dialog (empty ACTIVE section + scrollable
  AVAILABLE list, Audio:* rows visible at the top, detectors follow lower in the scroll), a clean modal over
  the real listing. No `-fail.png` produced.
- Verification: `npm run check` → 0 errors / 0 warnings; `gui-smoke` typecheck (`tsc --noEmit`) clean; branch
  diff is the spec + this ticket only — no app-code/Rust change (a build-touched `Cargo.toml` line-ending was
  reverted). Manual-verify debt for the ColumnPickerDialog is now retired; pinned by
  `gui-smoke/specs/column-picker.smoke.ts`.
