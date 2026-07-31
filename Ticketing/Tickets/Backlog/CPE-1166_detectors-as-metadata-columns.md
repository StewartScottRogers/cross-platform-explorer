---
id: CPE-1166
title: "Surface the true-type / text-encoding / line-ending detectors as opt-in metadata columns"
type: feature
component: Backend
priority: high
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-1000
---

## Summary
PM-scouted (2026-07-31). CPE-1001's `detect_type(bytes)->Option<FileType>` + `mismatch(bytes, ext)` (true-type /
extension-mismatch) and CPE-1003's `text_encoding` (encoding + line-endings) are **pure, cargo-tested, and wired
to nothing**. The generic metadata-column pipeline (epic CPE-707: CPE-1145/1146/1147, shipped 2026-07-30 —
*after* the last "headless well tapped" sweep) now lets us surface them as **opt-in columns with NO new GUI** —
the shipped `ColumnPickerDialog` enumerates whatever `MetaColumn::all()` exposes, and the streamed
`metadata_column_cells` command already carries them. Real user value (spot a `.jpg` that's actually a `.exe`;
a Latin-1 file about to be mangled), zero cost when the columns are off — dead-on the fast/small/predictable
tiebreaker.

## What exists (study first)
- `crates/server/src/column_extract.rs` — pure `extract_column(ext, bytes, MetaColumn) -> CellValue` dispatcher;
  `MetaColumn::all()` (same file) auto-feeds `column_cells::available_columns()` which the picker enumerates.
  (See the existing `routes_audio_*` tests for the test shape.)
- Detectors to call: CPE-1001 `detect_type` / `mismatch`, CPE-1003 `text_encoding` (find their modules in
  `crates/server`). All pure + already tested.
- `column_cells.rs` (`available_columns()` / streamed `metadata_column_cells`), `ColumnPickerDialog.svelte`,
  `src/lib/columns.ts`.

## Build
1. Add `MetaColumn` variants: **`TrueType`** (detected real type, or "" ), **`TypeMismatch`** (flag when detected
   type ≠ extension — a `CellValue::Text` like "mismatch: exe" or empty), **`TextEncoding`**, **`LineEndings`**.
   Each new arm in `extract_column` calls the existing detector and maps to `CellValue::Text` (reuse existing
   CellValue variants; don't invent new ones unless truly needed). Register all in `MetaColumn::all()` with a
   label + id.
2. **"Applies to all files" sentinel (the one design wrinkle — decide + implement headless):** these columns
   apply to ANY file, but `extensions()` is currently asserted non-empty and drives the picker's grey-out.
   Introduce an "applies to all" convention — **empty `extensions()` == applies to every file** — and update:
   the non-empty assertion/test, `available_columns()` gating, and the picker/`columns.ts` grey-out logic so an
   applies-to-all column is never greyed out. Keep it minimal + covered by tests.
3. Read only what the detectors need (they already cap their own reads; reuse `column_cells`' existing bounded
   header read — do NOT read whole files). Respect the existing truncation/`Empty`-on-partial behavior.
4. **specta/bindings:** adding `MetaColumn` variants (a `specta::Type`) changes the bindings — regenerate
   `src/lib/bindings.gen.ts` (`export PATH="$HOME/.cargo/bin:$PATH" && cargo run --bin export_bindings --features "specta-bindings sidecar-platform"`) or CI's drift guard reds.

## Acceptance Criteria
- [ ] New columns (True type, Type-mismatch, Text encoding, Line endings) appear in the ColumnPickerDialog and,
      when enabled, populate via the streamed `metadata_column_cells` for real files (true type detected;
      mismatch flagged; encoding + CRLF/LF shown).
- [ ] "Applies to all files" columns are never greyed out by the extension gate; extension-scoped columns
      unchanged. The non-empty-`extensions()` assertion is replaced by the sentinel + a test.
- [ ] cargo tests in `column_extract` cover each new column (a true-type sample, a mismatched sample, a
      UTF-8/Latin-1 + CRLF/LF sample), mirroring `routes_audio_*`; the generic jsdom picker test still passes.
- [ ] `npm run check` green; `cargo clippy --all-targets -- -D warnings` (default) AND `--features sidecar-platform`
      AND crate-level `cd crates/server && cargo clippy --all-targets -- -D warnings` all clean; `bindings.gen.ts`
      regenerated (no drift). No whole-file reads; opt-in = zero cost when off.

## Notes
- Epics CPE-1000 (file-type detection) + CPE-1002 (inspection) — closes their DoD "surface it" language.
- Backend-correctness ⇒ opus reviewer (per history.md seed default). One worker owns column_extract.rs +
  MetaColumn + the picker grey-out (single conflict surface) — no parallel worker on these files.
