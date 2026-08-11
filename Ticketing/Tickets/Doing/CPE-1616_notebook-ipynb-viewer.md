---
id: CPE-1616
title: "Jupyter .ipynb viewer — render cells (code + markdown + outputs), not raw JSON"
type: Feature
status: Doing
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

## Work Log

### 2026-08-11 — implemented, PR opened

Built the notebook viewer exactly to Scope:

- `src/lib/preview/notebook.ts` (+ `.test.ts`, 34 tests): pure `parseNotebook(raw)` parser. Never throws —
  invalid JSON, a non-object root, a missing/non-array `cells`, a non-object cell, a missing/wrong-shaped
  `source`/output field all degrade to a typed `{ok:false, error}` result. Every cap bounds WORK examined,
  not just what's shown: `cells`/`outputs` arrays are `.slice()`d to `MAX_CELLS`(300)/`MAX_OUTPUTS_PER_CELL`(20)
  **before** `.map()`, and a cell's source / an output's text is `.slice()`d to
  `MAX_CELL_SOURCE_CHARS`(100k)/`MAX_OUTPUT_TEXT_CHARS`(20k) before ever reaching markdown/highlight. An
  oversized `image/png` payload (`MAX_OUTPUT_IMAGE_CHARS`, ~3 MB decoded) is skipped, not embedded. Two
  timing-bound regression tests (40,000 cells / 40,000 outputs, generous 2s budget) stand in for a
  getter-based "never read" proof, which can't survive the string-in API (building that fixture requires
  `JSON.stringify`, which itself visits every getter before `parseNotebook` ever runs).
- `src/lib/components/NotebookPreview.svelte` (+ `.test.ts`, 7 tests): self-contained provider component
  (fetches its own content via `commands.readFileText` — routed through the generated typed client, which
  itself goes through `src/lib/invoke.ts`, never `@tauri-apps/api/core` directly). Renders cells in
  document order — markdown via the existing sanitized `renderMarkdown` (DOMPurify), code via a new
  `highlightCode(code, lang)` in `highlight.ts` (language from `metadata.kernelspec.language`/
  `language_info.name`, default `python`), raw as plain preformatted text. Outputs: stream (stdout/stderr,
  stderr styled via `--danger`), `execute_result`/`display_data` (`text/plain` + `image/png` as a `data:`
  URL), and `error` (styled distinctly, traceback shown). A parse failure degrades to the raw fetched text
  in a `<pre>` with a clear reason banner — never a blank pane. All CSS uses existing semantic theme
  tokens only (`--surface`, `--surface-alt`, `--border`, `--text`, `--text-dim`, `--text-faint`,
  `--danger`, `--radius`, `--accent`, `--mono` fallback) — no new tokens, so no WCAG guard-test additions
  were needed.
- `src/lib/preview/provider.ts` (shared file): added `"notebook"` to the `PreviewKind` union and one new
  provider entry (`canPreview: e => !e.is_dir && e.extension === "ipynb"`), inserted before the
  `json`/`text` providers per the ticket's ordering note. No other entries touched.
- `src/lib/components/PreviewPane.svelte` (shared file): one import (`import NotebookPreview from
  "./NotebookPreview.svelte";`) + one new `{:else if provider.kind === "notebook" && entry}` branch,
  inserted next to the other self-contained providers (cert/font/jwt). No other lines touched.
- `src/lib/preview/highlight.ts`: added `highlightCode(code, lang)` — the `highlightForFile` counterpart
  for a caller that already has a language id (not a file name), which a notebook cell needs since it has
  no filename/extension to resolve from. Reuses the same registered-grammar/escape-fallback logic.
- Sample-coverage ratchet (`src/lib/sampleCoverage.test.ts`) requires a real sample per registered preview
  kind, so added `samples/text/notebook.ipynb` — a real notebook with markdown + 3 code cells covering all
  four rendered output shapes (stream/execute_result/display_data image/error) + a raw cell. Generated
  deterministically from a new `TEXT_FILES["text/notebook.ipynb"]` entry in `scripts/gen_samples.py`
  (`json.dumps` of a Python dict, no ffmpeg/PIL dependency) and verified byte-identical to the checked-in
  file. Updated `samples/README.md`'s file table to document it.
- Fixed a pre-existing test in the shared `src/lib/preview/provider.test.ts` that hard-coded the OLD
  behavior (`.ipynb` → `"text"` kind, CPE-114) — split it into an HTML-only assertion plus a new
  `.ipynb` → `"notebook"` assertion, since that's exactly the behavior this ticket intentionally changes.

**Markdown sanitization**: notebook markdown cells render through the SAME `renderMarkdown()`
(`src/lib/preview/markdown.ts`) every other markdown surface in the app uses, which already runs `marked`
output through `DOMPurify.sanitize()` before returning. No new sanitization path was introduced or needed;
notebook markdown is no more of an injection vector than a plain `.md` file.

**Verification**:
- `npm run check` → `svelte-check found 0 errors and 0 warnings` (ran twice, clean both times).
- `npx vitest run` (full suite) → `Test Files 274 passed (274)`, `Tests 3355 passed (3355)`. First run
  surfaced 3 failures (2 test-construction bugs of mine — getter-based fixtures broken by
  `JSON.stringify`, fixed as above — and the pre-existing `.ipynb`→`text` assertion above); all three
  fixed, full re-run green.
- Targeted re-run of the 5 touched/added suites (`highlight.test.ts`, `notebook.test.ts`,
  `sampleCoverage.test.ts`, `provider.test.ts`, `NotebookPreview.test.ts`) → all green, 113/113.
- jsdom cannot see layout, so no visual claim is made — only parsing/robustness/DOM-content assertions.

Branch `cpe-1616-notebook-viewer`, PR opened against `main`.
