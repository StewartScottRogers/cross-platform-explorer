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
- [x] New columns (True type, Type-mismatch, Text encoding, Line endings) appear in the ColumnPickerDialog and,
      when enabled, populate via the streamed `metadata_column_cells` for real files (true type detected;
      mismatch flagged; encoding + CRLF/LF shown).
- [x] "Applies to all files" columns are never greyed out by the extension gate; extension-scoped columns
      unchanged. The non-empty-`extensions()` assertion is replaced by the sentinel + a test.
- [x] cargo tests in `column_extract` cover each new column (a true-type sample, a mismatched sample, a
      UTF-8/Latin-1 + CRLF/LF sample), mirroring `routes_audio_*`; the generic jsdom picker test still passes.
- [x] `npm run check` green; `cargo clippy --all-targets -- -D warnings` (default) AND `--features sidecar-platform`
      AND crate-level `cd crates/server && cargo clippy --all-targets -- -D warnings` all clean; `bindings.gen.ts`
      regenerated (no drift). No whole-file reads; opt-in = zero cost when off.

## Notes
- Epics CPE-1000 (file-type detection) + CPE-1002 (inspection) — closes their DoD "surface it" language.
- Backend-correctness ⇒ opus reviewer (per history.md seed default). One worker owns column_extract.rs +
  MetaColumn + the picker grey-out (single conflict surface) — no parallel worker on these files.

## Work Log
- 2026-07-31 (worker, branch `cpe-1166-detectors-as-columns`): implemented.

  **Columns added** — four new `MetaColumn` variants in `crates/server/src/column_extract.rs`, registered
  in `all()` after the media families, each mapped to `CellValue::Text` (no new `CellValue` variant needed):
  - `TrueType` (id `detect.true_type`, label "True Type") → `file_type::detect_type(bytes)` → the type's
    `label()` (e.g. "PNG image"), else `Empty`.
  - `TypeMismatch` (id `detect.type_mismatch`, label "Type Mismatch") → `file_type::mismatch(bytes, ext)` →
    a compact `"mismatch: <canonical-ext>"` flag using the detected type's first canonical extension (a PE
    disguised as `.jpg` → `"mismatch: exe"`), else `Empty` (agrees / unknown / no extension).
  - `TextEncoding` (id `detect.text_encoding`, label "Text Encoding") → `text_encoding::detect_encoding` →
    the guess's `label()` ("UTF-8" / "Latin-1 / 8-bit (guessed)" / "Binary" / …); a zero-byte file → `Empty`
    (not the "Empty file" label, so blanks stay consistent + sort last).
  - `LineEndings` (id `detect.line_endings`, label "Line Endings") → new private `line_endings_cell()`
    mirroring `inspect.rs`: binary/empty and break-less text → `Empty`; else "LF (Unix)" / "CRLF (Windows)" /
    "CR (classic Mac)" / "Mixed".

  All four **call the existing pure, cargo-tested detectors** (CPE-1001 / CPE-1003) — no reimplementation.

  **Applies-to-all sentinel decision** — the detectors apply to *any* file, but `extensions()` was asserted
  non-empty and drives the picker's grey-out. Convention adopted: **empty `extensions()` == applies to every
  file**. The four detector variants return `&[]`; added `MetaColumn::applies_to_all()` (= extensions empty)
  documenting it. Replaced the old "extensions non-empty for all" assertion with a dedicated sentinel test
  (`applies_to_all_sentinel_is_empty_extensions_for_detectors_only`) that asserts the detectors are empty +
  applies-to-all while media columns stay extension-scoped. `available_columns()` needs no gating change — it
  already maps `extensions()` straight to the wire, so an applies-to-all column simply ships an empty list;
  its doc + the `AvailableColumn` doc now record the sentinel. Frontend: `src/lib/columns.ts` gains
  `appliesToAllFiles(extensions)` + `columnAppliesTo(extensions, ext)` (mirroring the Rust helper) so any
  extension-gate consumer treats empty-extensions as "matches every file" → never greyed. (The current
  `ColumnPickerDialog` only disables Add for already-active columns, so an applies-to-all column was never
  greyed regardless; a jsdom test now pins that a detector column with `extensions: []` renders with an
  enabled Add button.)

  **Bounded read preserved** — no code path reads whole files. Cells flow through the unchanged
  `column_cells.rs` capped 1 MiB header read; the DocPages truncation→Empty special-case is untouched. The
  detectors cap their own scans internally. Line-endings/encoding over a truncated header are documented as a
  per-row *sample* (never a "wrong count" like DocPages), so no new truncation handling was required.

  **Verification** (all from the worktree):
  - `cd crates/server && cargo test column_extract` → 17 passed (incl. the four new detector routing tests +
    the sentinel test). `cargo test column_cells` → 6 passed. Full `cargo test` → 1096 passed, 0 failed.
  - `cargo clippy --all-targets -- -D warnings` (crate cpe-server, default AND `--features index`) → clean.
  - `src-tauri`: `cargo clippy --all-targets -- -D warnings` (default) → clean; `--features sidecar-platform`
    → clean.
  - `npm run check` → 0 errors / 0 warnings. `vitest` on columns.test.ts (24) + ColumnPickerDialog.test.ts
    (9) + ExplorerPane.metaColumns.test.ts (3) → 36 passed.
  - `bindings.gen.ts` regenerated via `cargo run --bin export_bindings --features "specta-bindings
    sidecar-platform"` (adds the 4 `MetaColumn` string variants + the `AvailableColumn` doc); a second run
    produced an identical diff → **no drift**.

  Scope: `crates/server` (column_extract.rs, column_cells.rs) + `src/lib/bindings.gen.ts` +
  `src/lib/columns.ts` / columns.test.ts + ColumnPickerDialog.test.ts. Ticket left in Backlog per
  instructions.
