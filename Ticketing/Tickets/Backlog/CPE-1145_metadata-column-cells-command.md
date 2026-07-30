---
id: CPE-1145
title: "Metadata columns: streamed command to extract a chosen column's typed cells for a listing (backend enablement)"
type: feature
component: Backend
priority: high
status: Backlog
tags: ready
created: 2026-07-30
epic: CPE-707
---

## Summary
The metadata-column engine is built (`crates/server/src/column_extract.rs` `extract_column(ext, bytes,
MetaColumn)`; `metadata_column::CellValue` + compare/sort/display; per-family extractors audio/image/video/doc
— CPE-918/971/974/975/1028/1029) but wired to **zero commands**. This slice exposes it so the frontend
column-picker (CPE-1146) can populate + sort metadata columns. Backend only.

## Design (thin command into cpe-server; async + spawn_blocking; streamed)
- **Enumerate available columns.** Add serde + gated `specta::Type` to `MetaColumn` (variants: `Audio`,
  `ImageDimensions`, `VideoDuration`, `VideoTag`, `DocPages`, `DocInfo`) and a small `metadata_columns_available()`
  helper/command returning the pickable columns with a stable id + a display label + which extensions each
  applies to (so the picker can show human labels + grey out non-applicable rows). Keep the label/ext mapping
  in `cpe-server` (domain), not the app.
- **Extract cells (streamed).** A command `metadata_column_cells(paths: Vec<String>, column: MetaColumn, on_cell: Channel<...>)`
  that, for each path, reads a **capped header** of the file (reuse the existing header-read helper the
  extractors already assume — grep how audio/image extractors get their bytes; header-only, never whole file)
  and runs `extract_column`, streaming `{ path, cell: CellValue }` back in batches per `docs/design/STREAMING.md`
  (so the list paints visible rows fast). Provide a collect-to-vec variant for tests. Skip-on-error per file
  (an unreadable/undecodable file → a `CellValue::Empty`/None, never fatal).
- `CellValue` already has `compare`/`display` — expose whatever the frontend needs to sort + format (the wire
  type + a `display` the command can pre-format, OR ship the typed value and let the frontend format; pick the
  simpler one and note it). Ensure `CellValue` is serde+specta so it crosses IPC.
- **Regenerate `src/lib/bindings.gen.ts`** (`cargo run --bin export_bindings --features "specta-bindings
  sidecar-platform"`) — new specta commands/types → CI drift guard ([[regen-specta-bindings-on-struct-change]]).
- Off/lean: this is opt-in (only called when a metadata column is active); no cost when the picker adds nothing.

## Acceptance Criteria
- [ ] `metadata_columns_available()` returns the pickable columns (id + label + applicable extensions).
- [ ] `metadata_column_cells(paths, column)` streams a typed cell per path via `ipc::Channel` (+ a
      collect-to-vec variant for tests); reads only a capped header; skip-on-error → empty cell, never panics.
- [ ] `MetaColumn` + `CellValue` are serde+`specta` and appear in `bindings.gen.ts` (regenerated + committed);
      `npm run check` green.
- [ ] `crates/server` tests (incl. a new test: a small fixture set → correct typed cells per column, sorted
      via `CellValue::compare`) + `cargo clippy --all-targets -- -D warnings` green; `src-tauri` `cargo check` green.

## Notes
- Prereq for CPE-1146 (the column-picker UI + dynamic FileList columns + per-folder persistence).
- Header-only + streamed + visible-rows-only keeps it cheap (coordinate w/ virtualization CPE-690) — the
  extractors are already header-only reads.
