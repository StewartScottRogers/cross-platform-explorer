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
- [x] `metadata_columns_available()` returns the pickable columns (id + label + applicable extensions).
- [x] `metadata_column_cells(paths, column)` streams a typed cell per path via `ipc::Channel` (+ a
      collect-to-vec variant for tests); reads only a capped header; skip-on-error → empty cell, never panics.
- [x] `MetaColumn` + `CellValue` are serde+`specta` and appear in `bindings.gen.ts` (regenerated + committed);
      `npm run check` green.
- [x] `crates/server` tests (incl. a new test: a small fixture set → correct typed cells per column, sorted
      via `CellValue::compare`) + `cargo clippy --all-targets -- -D warnings` green; `src-tauri` `cargo check` green.

## Notes
- Prereq for CPE-1146 (the column-picker UI + dynamic FileList columns + per-folder persistence).
- Header-only + streamed + visible-rows-only keeps it cheap (coordinate w/ virtualization CPE-690) — the
  extractors are already header-only reads.

## Work Log (2026-07-30)

**Domain (`cpe-server`):**
- `column_extract.rs`: `MetaColumn` (+ the nested `AudioColumn`/`DocInfoColumn`/`VideoTagColumn`) now
  derive `serde::{Serialize, Deserialize}` + gated `specta::Type`. Added `MetaColumn::all()` (32 columns:
  12 audio + 1 dimensions + 1 pages + 8 doc-info + 1 duration + 9 video-tag), `.id()` (stable
  family-prefixed snake_case string, e.g. `"audio.track"`, `"doc.info.title"` — for future
  `column_config` persistence, which intentionally stores string ids decoupled from this enum),
  `.label()` (family-prefixed friendly text, e.g. `"Audio: Year"` vs `"Video: Year"`, so recurring names
  are unambiguous in a flat picker list), and `.extensions()`. Refactored the existing
  `is_image_ext`/`is_doc_ext`/`is_video_ext` gates to share the same `AUDIO_EXTS`/`IMAGE_EXTS`/
  `DOC_EXTS`/`VIDEO_EXTS` const arrays instead of duplicating the extension lists.
- New module `column_cells.rs` (registered in `lib.rs`): the file-I/O layer on top of the pure
  `extract_column`. `available_columns()` wraps `MetaColumn::all()` into `AvailableColumn { id, label,
  column, extensions }`. `stream_column_cells(paths, col, batch, flush)` is the shared walker (capped
  header read → `extract_column` → `MetadataCell { path, cell, display }`, flushed in batches via a
  `ControlFlow` closure, mirroring `content_search::stream_file_contents`); `column_cells(paths, col)` is
  its collect-to-vec wrapper.
- **Header cap: 1 MiB (`HEADER_CAP = 1_048_576`).** No existing shared "capped header read" helper was
  found in `cpe-server` (the ticket's Study-first pointer assumed one existed; the closest precedent is
  `binary_preview::hex_dump`'s inline `file.take(max).read_to_end`, and `media_meta_read::read_pdf`'s own
  internal last-resort scan already self-limits to `bytes.len().min(1_048_576)`). 1 MiB was chosen to
  match that existing internal PDF bound exactly, so raising the cap further wouldn't help PDFs beyond
  what `read_pdf` already self-limits to; ID3/FLAC/OGG tags and image headers resolve from far less. A
  `moov` box at the end of a non-"faststart" MP4, or PDF page objects beyond 1 MiB, won't be found — an
  accepted trade-off for a per-row column fill, documented in the code.
- **CellValue-over-IPC decision (ship both):** rather than choosing typed-only vs pre-formatted-only,
  `MetadataCell` carries both the typed `cell: CellValue` (future type-aware frontend behaviour, e.g.
  right-aligning numbers) *and* a pre-formatted `display: String` — reusing the existing
  `CellValue::display("—")` Rust method rather than making CPE-1146's frontend reimplement byte/float/
  dimension formatting in TypeScript. This was simpler than either single-sided option since it required
  zero new formatting logic (the method already existed) while still shipping the typed value the AC asks
  for.
- Skip-on-error: `read_header` returns any I/O error (missing file, permission, a directory, …) as `Err`;
  `stream_column_cells` treats that as an empty header (`unwrap_or_default()`), which `extract_column`
  already turns into `CellValue::Empty` for every family (verified by existing `column_extract` fixture
  tests) — never a panic, never an aborted batch.

**App adapter (`src-tauri/src/lib.rs`):**
- `metadata_columns_available() -> Vec<AvailableColumn>` — sync, no I/O, thin dispatcher (mirrors
  `index_status`'s in-memory-only sync-fn pattern).
- `metadata_column_cells(paths: Vec<String>, column: MetaColumn, on_cell: Channel<Vec<MetadataCell>>) ->
  Result<usize, String>` — async + `spawn_blocking`, streams batches, returns the total cells emitted
  (mirrors `index_search`).
- `metadata_column_cells_collect(paths: Vec<String>, column: MetaColumn) -> Vec<MetadataCell>` — async +
  `spawn_blocking` collect-to-vec variant for tests/non-streaming callers (mirrors `index_search_collect`).
- Registered in both the `generate_handler!` and `collect_commands!` macro lists.
- Did not add a `require_local` guard: the single-path `metadata_read`/`metadata_write` commands don't
  guard either, and a non-local path here would just fail `std::fs::File::open` and degrade to an empty
  cell like any other unreadable path — consistent with the skip-on-error design.

**Bindings:** regenerated via `cargo run --bin export_bindings --features "specta-bindings
sidecar-platform"` (from `src-tauri`) and committed — `MetaColumn`, `CellValue`, `AvailableColumn`,
`MetadataCell`, and the three new commands (`metadataColumnsAvailable`, `metadataColumnCells`,
`metadataColumnCellsCollect`) now appear in `src/lib/bindings.gen.ts`.

**New tests:** 17 new tests across `column_extract.rs` (id/label/extensions coverage: uniqueness across
all 32 columns, stable id tokens, label disambiguation) and the new `column_cells.rs` (real on-disk
fixtures — an ID3 mp3, a real PNG via the `image` crate, a synthetic PDF — through `column_cells()`
end-to-end, asserting typed cells + pre-formatted `display` + `CellValue::compare` numeric-not-lexical
sorting (track 9 before 10) + skip-on-error for a missing file and a non-matching extension + streaming
batch/early-break behaviour + `available_columns()` shape).

**Verify:** `crates/server`: `cargo test` 1081/1081 passed (incl. the 17 new), `cargo clippy --all-targets
-- -D warnings` clean, `cargo check --features specta` clean. `src-tauri`: `cargo check --features
"specta-bindings sidecar-platform"` clean, `cargo clippy --features sidecar-platform -- -D warnings`
clean. `npm run check`: 0 errors, 0 warnings.

**Assumptions:** no GUI/frontend wiring in this slice (CPE-1146 owns the picker UI); the string `id` field
is speculative plumbing for `column_config` (CPE-1032) persistence, not yet wired to it (that store still
takes arbitrary strings, so no change was needed there); `require_local` intentionally omitted (see
above).
