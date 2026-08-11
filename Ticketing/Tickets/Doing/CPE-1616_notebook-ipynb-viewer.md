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

**Correction (2026-08-11, before the visual-fix pass below):** the "code cells syntax-highlighted via
`highlight.ts`" claim above is misleading as stated. `highlightCode`/`highlightForFile` genuinely run
highlight.js and emit `hljs-*`-classed markup, but **no stylesheet anywhere in the app defines any
`.hljs-*` rule**, so on screen every code cell (notebook or plain-text preview) renders flat monochrome in
both themes — a pre-existing, app-wide gap this ticket didn't cause, just made visible (a notebook is
mostly code). Filed separately as **CPE-1631**; not fixed here. PR description corrected to match.

### 2026-08-11 — Visual Critic fixes (blocking findings on PR #822)

An independent Reviewer approved the code (caps genuinely bound work, sanitisation is sound, parsing
never throws — see above, unchanged). A separate Visual Critic then looked at the real rendered component
in Chrome, both themes, 900/460/260px, and returned 4 findings. Fixed:

- **Finding 1 (must-fix) — ANSI escape codes rendered as literal garbage.** A real Jupyter
  traceback/stream is routinely colourised by the kernel (IPython's exception formatter, `colorama`,
  `tqdm`, …) with raw ANSI escape codes; unstripped, the view showed fragments like `[0;31m` interleaved
  with the real message. Added `stripAnsi()` to `src/lib/preview/notebook.ts` — a small inline regex (no
  new dependency; same shape as the `strip-ansi` npm package, reimplemented rather than added), applied to
  stream text, `text/plain` results, and error tracebacks, run BEFORE the existing `MAX_OUTPUT_TEXT_CHARS`
  cap (so the cap reflects what's actually rendered, not the raw escape-code-inflated source). **Chose
  stripping over colourising**: rendering real ANSI colour would need switching the traceback `<pre>` to
  `{@html}` plus careful sanitisation of attacker-controlled SGR parameters to stay safe — a "nice to
  have" not worth the injection-surface risk for what this ticket actually needs (a readable traceback).
  Stripping keeps the existing safe `<pre>{text}</pre>` (auto-escaped) untouched. 9 new tests in
  `notebook.test.ts` (direct `stripAnsi` unit tests + `parseNotebook` integration for stream/result/error,
  including a real-shaped multi-line traceback fixture with actual `\x1b[` sequences) + 3 new tests in
  `NotebookPreview.test.ts` (end-to-end: traceback and stream rendering with ANSI stripped).
- **Finding 2 (must-fix) — unbounded output height buries the notebook.** `.nb-output` (covers stream,
  result text, AND the error/traceback block, which shares the class) had no `max-height`, so e.g. a
  300-line stream (well within the 20,000-char text cap, so never marked `truncated`) rendered every line
  inline, forcing a scroll through the whole dump to reach the next cell. Added `max-height: 260px;
  overflow-y: auto; resize: vertical` to `.nb-output` in `NotebookPreview.svelte`. Short outputs are
  unaffected (never reach the height cap, no scrollbar appears). The `resize: vertical` handle is a
  deliberate, visible "there's more, drag me" affordance (native browser corner-drag icon) on top of the
  native scrollbar, so the bound reads as designed, not as silent truncation — and nothing is actually cut:
  every byte within the existing text-length cap stays in the DOM. Added a DOM-content test proving a
  300-line output's full text (first AND last line) is still present — the data-integrity half of the fix
  that jsdom *can* verify; the visual bound itself (an actual `max-height`/scrollbar rendering) needs the
  screenshot/Visual-Critic pass, same as every other layout claim in this suite — plus a structural guard
  test that reads the component's own `<style>` block and asserts the `.nb-output` rule declares
  `max-height`/`overflow-y: auto`.
- **Finding 3 (fix while here) — dark-theme traceback contrast 4.41:1, under the 4.5:1 AA floor.** The
  traceback's real background is `color-mix(in srgb, var(--danger) 8%, var(--surface))` (`.nb-error-output`
  in `NotebookPreview.svelte`), not plain `--surface` — the existing `app.css.dark-contrast.test.ts` guard
  checks `--danger` against plain `--bg`/`--surface` (4.92:1, already fine) but not this component's actual
  mixed background. Nudged `--pal-dark-red-400` (the dark `--danger` token, `src/app.css`) from `#ff6659`
  to `#ff7b6f`. **Measured after the change: 4.97:1** against the real 8%-mixed traceback background
  (up from 4.41:1), 5.61:1 against plain `--surface` (up from 4.92:1), 6.45:1 against `--bg`. Added a
  dedicated regression guard in `NotebookPreview.test.ts` that reads the live hex values out of `app.css`
  and reimplements `color-mix` in JS (same file-local WCAG-math convention as
  `app.css.dark-contrast.test.ts`/`app.css.hc-contrast.test.ts` — no shared util exists yet) so this can't
  silently drift back under 4.5:1. Light theme was already fine (4.99:1), untouched. The existing
  `app.css.dark-contrast.test.ts` `--danger` assertions still pass (now 5.61:1/6.45:1, both improved).
- **Finding 4 (do not fix here) — "no syntax highlighting" claim corrected, not the underlying gap.**
  Confirmed via `grep -r "\.hljs" src/` → no matches anywhere in the app; the Critic's finding is accurate
  and pre-existing/app-wide, tracked as CPE-1631 (already filed on `main`). Left the highlight.js
  integration itself untouched. Corrected: this ticket's Work Log entry above, `NotebookPreview.svelte`'s
  top doc comment (now states plainly that highlighting is currently invisible + points at CPE-1631), and
  the PR #822 description (removed the "syntax-highlighted" claim, added a CPE-1631 reference).

**Verification:**
- `npm run check` → `svelte-check found 0 errors and 0 warnings`.
- `npx vitest run` (full suite) → `Test Files 274 passed (274)`, `Tests 3369 passed (3369)` — up from the
  274/3355 baseline by exactly the 14 new tests added here (9 in `notebook.test.ts`, 5 in
  `NotebookPreview.test.ts`), no existing test touched or weakened.
- Targeted re-run of `notebook.test.ts` + `NotebookPreview.test.ts` → 55/55 green; `app.css.dark-contrast
  .test.ts` + `app.css.hc-contrast.test.ts` + `app.css.test.ts` → 38/38 green (dark-contrast `--danger`
  assertions improved, not regressed).
- No new npm dependency added (`package.json`/`package-lock.json` untouched) — the ANSI stripper is a
  ~10-line inline regex.
- Still needs an actual screenshot/Visual-Critic re-pass to confirm the max-height/scroll bound and the
  now-brighter dark-theme red *look* right on screen — jsdom cannot see layout or colour, only the
  DOM-content and CSS-declaration/contrast-math halves of these fixes were verified here.

Pushed to `cpe-1616-notebook-viewer`; PR #822 updated (same PR, not a new one).

### 2026-08-11 — correction: revert the shared token move, fix the traceback locally instead

Finding 3's fix above nudged **`--pal-dark-red-400`** — the dark-theme `--danger` token in
`src/app.css`, a token backing ~80 files app-wide, not something scoped to this ticket. A follow-up
Visual Critic re-check measured the knock-on effect on a surface nobody was looking at: white text on a
**solid** `--danger` background (`ConfirmDialog`/`ShredConfirmDialog`/`CheckpointDialog`/
`BatchMediaDialog`'s primary destructive buttons, `.agent-badge.removed`/`.tl-badge.removed` pills,
`Sidebar`'s `.drive-bar-fill.full`) — white on `#ff6659` measured 2.88:1, white on the nudged `#ff7b6f`
measured 2.53:1. Both already fail WCAG's 3:1 floor for non-text UI (a pre-existing gap, filed separately
as **CPE-1632**, not fixed here), but the nudge measurably widened the gap and made the app-wide
destructive red read visibly softer/more pastel everywhere it's used — a notebook ticket should not move
an app-wide theme token, and definitely shouldn't leave an unrelated destructive-action surface worse
than it found it.

**Fix:**
- Reverted `--pal-dark-red-400` (`src/app.css`) back to its original `#ff6659`. Confirmed via `grep` that
  it's the ONLY change to the shared token layer — no other palette/semantic token moved.
- Added a new token, **`--danger-on-tint`**, scoped to "text that must render on a
  `color-mix(--danger, --surface)`-tinted background" rather than a plain surface/bg. Defined in all
  five theme blocks (bare `:root` fallback, `light`, `dark`, `hc-light`, `hc-dark`) per the app's
  three-tier palette/semantic convention — required by `app.css.dark-contrast.test.ts`'s symmetric
  token-completeness check (a token in one theme block but not the others fails that guard). Resolves to
  plain `--danger` in every theme except dark, where it resolves to a new dark-only palette primitive
  `--pal-dark-red-450-notebook` (`#ff7b6f` — the same hex the reverted shared-token nudge used, just no
  longer shared). Currently consumed ONLY by `NotebookPreview.svelte`'s `.nb-error-output` — not a
  general-purpose danger-text token, and the doc comments on both the new palette primitive and the new
  semantic token say so explicitly, to discourage a future reach-for-it-anywhere.
- `NotebookPreview.svelte`'s `.nb-error-output` rule now reads `color: var(--danger-on-tint)` instead of
  `color: var(--danger)`. The background (`color-mix(in srgb, var(--danger) 8%, var(--surface))`) and
  border (`var(--danger)`) are unchanged — only the text colour is scoped locally.
- Retargeted the dedicated contrast guard in `NotebookPreview.test.ts` from `--danger` to
  `--danger-on-tint`, generalized its hex-resolver to follow either one or two hops of `var()`
  indirection (needed because light theme's `--danger-on-tint` aliases back to `--danger`, itself a
  palette reference — two hops — while dark theme's resolves straight to the new palette primitive — one
  hop), and added a companion light-theme assertion (light was always fine and is unchanged by this fix,
  but the ticket asked for both themes measured/pinned, and the light guard costs nothing extra to keep
  it honest against regression).

**Measured ratios (reading live app.css values, same `color-mix` reimplementation as the existing
`app.css.dark-contrast.test.ts`/`app.css.hc-contrast.test.ts` guards):**
- Dark theme: `--danger-on-tint` (`#ff7b6f`) on the real 8%-mixed background (`#3c302f`, mixed from the
  now-reverted `--danger` `#ff6659`) = **5.02:1** — clears the 4.5:1 AA floor, and with more margin than
  the original shared-token nudge had (4.97:1 against a background that, at the time, was ALSO mixed from
  the nudged red; now that the background reverts to the darker original red, the same lighter text
  measures slightly better against it).
- Light theme (untouched): `--danger-on-tint` == `--danger` (`#c42b1c`) on its 8%-mixed background
  (`#faeeed`) = **4.99:1**, unchanged from before this whole finding was ever raised.
- jsdom cannot see colour or layout — these are computed from the literal hex values app.css declares,
  not a rendered screenshot. The on-screen result (does the traceback text still read clearly, does the
  destructive-button red look right again) still needs the Visual Critic's eyes, same as every other
  colour claim in this ticket.

**Verification:**
- `npm run check` → `svelte-check found 0 errors and 0 warnings`.
- Targeted run (`NotebookPreview.test.ts` + `app.css.dark-contrast.test.ts` + `app.css.hc-contrast.test.ts`
  + `app.css.test.ts`) → 51/51 green, including both new/retargeted contrast assertions.
  - Caught and fixed one thing myself before it became a CI surprise: the first draft of the new
    `.nb-error-output` doc comment in `NotebookPreview.svelte` quoted the literal hex `#ff6659` in prose —
    `app.css.test.ts`'s hard-coded-hex ratchet regexes the whole file (doesn't strip comments), so that
    literal counted as a new hex occurrence and briefly broke the ratchet (91 files vs. the 90 baseline).
    Reworded the comment to say "hex ff6659" (no `#`) instead — no functional change, comment-only.
- `npx vitest run` (full suite) → **`Test Files 274 passed (274)`, `Tests 3370 passed (3370)`** — up from
  the 274/3369 baseline by exactly +1 (the new light-theme companion contrast assertion; the dark one was
  retargeted in place, not added). No existing test touched or weakened.
- Cross-reference: the pre-existing white-on-solid-danger contrast failure this correction surfaced (but
  did not introduce and does not fix) is tracked separately as **CPE-1632**.

Pushed to `cpe-1616-notebook-viewer`; PR #822 updated (same PR, not a new one). Ticket left in `Doing/`.
