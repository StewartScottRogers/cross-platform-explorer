---
id: CPE-1616
title: "Jupyter .ipynb viewer — render cells (code + markdown + outputs), not raw JSON"
type: Feature
status: Backlog
priority: Medium
component: Frontend
epic: CPE-1568
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Epic CPE-1568 (custom per-file-type right pane) slice 6, and confirmed a genuine zero-coverage gap by
reading the actual code: `LANG_BY_EXT` in `src/lib/preview/highlight.ts` maps `ipynb: "json"` — a
notebook today just renders as syntax-highlighted raw JSON text via the generic `text` provider. There is
no notebook-shaped provider anywhere in `src/lib/preview/provider.ts` (verified: no `ipynb`/`notebook` id
in the provider list). A `.ipynb` file is plain JSON (cells + outputs), so this is a **pure frontend**
slice — no backend command needed, matching the epic's "pure-JS cell render" plan.

## Goal
Selecting a `.ipynb` file shows its notebook structure — an ordered list of cells, each rendered per its
`cell_type`, instead of one undifferentiated JSON blob.

## Scope
**Conflict surface:** new `src/lib/preview/notebook.ts` (+ `.test.ts`) and new
`src/lib/components/NotebookPreview.svelte` (+ `.test.ts`); one new entry in
`src/lib/preview/provider.ts`'s provider array; one new `{:else if provider.kind === "notebook" && entry}`
branch + import in `src/lib/components/PreviewPane.svelte`. **This ticket, CPE-1617, and CPE-1618 all add
one array entry to `provider.ts` and one else-if branch to `PreviewPane.svelte` — each addition is small
and purely additive (new entry appended, not editing existing ones), but expect a rebase against
whichever of the three merges first; the Foreman should land them one at a time rather than assuming a
clean 3-way parallel merge on those two shared files.** No backend/`cpe-server` changes.

- Add a **pure module** `notebook.ts`: parse the `.ipynb` JSON (`{cells: [...], metadata, nbformat}`) into
  a typed shape, tolerant of a malformed/non-notebook JSON file (return a parse error, never throw past
  the caller — same discipline as `parseJson`/`formatJson` in the existing `json`-provider actions).
- Build `NotebookPreview.svelte` as a **self-contained provider component** (fetch its own file content
  from `path`, mirroring `BinaryPreview.svelte`'s/`CertPreview.svelte`'s pattern — not routed through
  `PreviewPane`'s shared text-loading state) that renders, per cell in document order:
  - `markdown` cells — reuse the existing `marked`-based renderer already used for the markdown provider
    (`src/lib/preview/markdown.ts`) so notebook markdown looks like everywhere else in the app.
  - `code` cells — reuse `src/lib/preview/highlight.ts` for syntax highlighting (Python by default per
    `metadata.kernelspec.language`, falling back sanely when absent), plus render any attached
    **outputs**: `stream` (stdout/stderr text), `execute_result`/`display_data` (`text/plain`, and
    `image/png` as an inline `data:` image — that's the only rich MIME type worth handling for v1), and
    `error` (traceback, styled distinctly so a failed cell reads as failed).
  - `raw` cells — plain preformatted text.
- Register the `notebook` provider in `provider.ts`: `canPreview: (e) => !e.is_dir && e.extension ===
  "ipynb"`, declared **before** the generic `text`/`json` providers claim it (same "declared before the
  generic providers" precedent the JSON/CSV comment in `provider.ts` already documents).
- Cap rendering for a pathologically large notebook (hundreds of cells / huge output blobs) so the pane
  never stalls — reuse the same capped-list + "showing N of M" pattern `binaryInspector.ts`'s `capRows`
  established, applied to the cell list.

## Explicitly NOT in scope
- No cell execution, no editing cells in place (read-only viewer, matching every other typed preview).
- No support for MIME types beyond `text/plain` and `image/png` in outputs — document the honest limit
  rather than half-rendering `text/html`/`application/json` rich outputs.

## Acceptance criteria
- A real `.ipynb` (mixed markdown + code + text/image outputs) renders as a cell-by-cell view, not raw
  JSON.
- A malformed/truncated/non-notebook `.json`-shaped file renamed to `.ipynb` shows a clear parse error,
  never a crash or blank pane.
- A notebook with hundreds of cells stays responsive (capped rendering, honest "showing N of M" note).
- `npm run check` and the new Vitest suites green; add a `sectionDocs.ts`-style doc note only if this ships
  behind a user-visible Section (it doesn't — it's a preview provider, same category as CSV/JSON, no new
  doc page required per CPE-579's own scope, which covers Sections not preview kinds).

## Notes
Model: sonnet. Library entry: `filetype-right-pane-coverage-2026-08-10` (epic spike).
