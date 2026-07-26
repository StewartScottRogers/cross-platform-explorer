---
id: CPE-1091
title: "Code preview: per-line rows + line/fold gutter, indent guides, minimap"
type: feature
component: Frontend
priority: high
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-724
depends-on: CPE-1090
---

## Summary
Child of CPE-724, GUI slice #2 of 2 — the visibility-heavy half. Refactor the code preview's single
highlight.js blob into **per-line rows** so we can attach a **line-number gutter**, a **fold gutter**
(collapsible ranges), **per-line indent guides**, and a synchronized **minimap** — all driven by the
already-merged `codeIntel` command (`{ outline, folds, indent, minimap }`). Builds on CPE-1090 (outline strip,
same file). Frontend only. Design de-risked by research — see the Library entry
`.claude/research-library/entries/hljs-blob-to-per-line-rows.md` (READ IT — it settles the hard part).

## Chosen approach (proven — do NOT re-explore)
Keep one-blob highlighting; add a **pure span-safe splitter** and render **one row per source line**. NOT a
tokenizer rewrite, NOT per-line re-highlighting, NOT a CSS-only overlay (an overlay cannot fold — collapsing a
run of lines out of a single blob is impossible; fold is the forcing function for a real DOM split).

## Context (verified — file:line)
- `src/lib/preview/highlight.ts`: `highlight.js/lib/core` (line 11); `highlightForFile(code,name)` (189-195)
  = one call `hljs.highlight(code,{language}).value` → a single HTML string; multi-line constructs emit a
  `<span>` whose content contains raw `\n` (span NOT closed at line boundaries — the crux). `escapeHtml`
  (180-182) is the pre-grammar fallback only.
- `PreviewPane.svelte`: `renderCode` (178-183) two-phase (escaped then re-highlight after grammar loads,
  `codeReq` gen-token). Code injected at ~`:459` (post-CPE-1090) as
  `<pre class="preview-text" class:nowrap={!wrapLines}><code>{@html codeHtml}</code></pre>`. `.preview-text`
  (~507-516) is `pre-wrap`, switches to `pre; overflow-x:auto` when `wrapLines` off (CPE-565 toggle). CPE-1090
  added the outline strip above the `<pre>` + `src/lib/preview/outline.ts` (`resolveLineHeight`,
  `lineToScrollTop`, `scrollTopToLine`, `enclosingSymbol`) — REUSE these for scroll/minimap sync.
- Bindings (`src/lib/bindings.gen.ts`): `CodeIntel = {outline, folds: FoldRange[], indent: number[], minimap:
  MinimapRow[]}`; `FoldRange {start_line,end_line,kind}` 1-based inclusive, kind ∈ block|suite|section;
  `indent: number[]` one-per-source-line; `MinimapRow {fill:0-255, indent}` bucketed per row;
  `codeIntel(text,lang,tabWidth,minimapBuckets)`.

## Design (buildable)
1. **Pure splitter** — new `export function splitHighlightedIntoLines(html: string): string[]` in
   `src/lib/preview/highlight.ts` (or a sibling `codeLines.ts`), string→string[], NO DOM (unit-testable like
   `outline.ts`). Single linear scan tracking the open-`<span>` stack: at each `\n`, close every open span,
   end the line; on the next line, reopen the same stack. **Scan only literal `<span`/`</span>`/`\n` tokens**
   over the already-escaped HTML (hljs emits `&lt;`/`&gt;`/`&amp;` — never raw angle brackets outside those,
   so entity text is safe). Handles: empty input (→ `[""]` or `[]`, pick one and test it), trailing newline,
   nested spans, a span that opens and closes mid-line, a comment/string spanning many lines.
2. **Per-line render** — in the code branch only (json/markdown/text panes untouched), compute
   `codeLines = splitHighlightedIntoLines(codeHtml)` and replace the single `{@html codeHtml}` with a
   `{#each codeLines as line, i}` of rows **inside one `<pre>`/`<code>`** (rows are `<span class="cl-row">` /
   block elements, NOT multiple `<code>` elements — keep select-all/copy yielding the whole file as one blob;
   verify `menuSelectAll`/copy still grabs the full text). Each row carries `data-line={i+1}`.
3. **Line-number + fold gutter** — a gutter column left of the rows. Line numbers per row. For each
   `FoldRange`, render a fold toggle on its `start_line` row; clicking collapses rows `start_line+1..end_line`
   (hide them + show a "⋯ N lines" affordance), click again expands. Fold state is per-file, reset on entry
   change. Gutter uses theme vars only.
4. **Indent guides** — per row `i`, draw `indent[i]` vertical guide(s) as a cheap background (e.g.
   `background-image` repeating vertical lines sized to `tab_width`·ch, or thin absolutely-positioned marks),
   depth from `CodeIntel.indent[i]`. Theme-var colour, subtle.
5. **Minimap** — a narrow column (right side) rendered from `MinimapRow[]` (one mark per bucket, opacity/width
   from `fill`, x-offset from `indent`) — a small `<canvas>` or a stack of divs. A viewport indicator box
   positioned/sized from the pane's `scrollTop`/`clientHeight` vs `scrollHeight` (reuse `resolveLineHeight` /
   the CPE-1090 scroll sync). Clicking/dragging the minimap scrolls the pane (map click-y → line → scrollTop
   via `lineToScrollTop`). Keep it optional/togguable if it crowds narrow panes.
6. **Compose with nowrap** — the gutter/fold/minimap assume uniform line height, which breaks under the wrap
   toggle (one source line → multiple visual rows). Force/hide the wrap toggle (or disable guides+minimap)
   whenever this code-intel view is active; document that it composes with the `nowrap` branch. A `data-line`
   row still works even if a row wraps taller, but the minimap/viewport math needs nowrap — gate accordingly.

## ⚠ Notes / guardrails
- No new deps (no CodeMirror/Monaco/Shiki, no `highlightjs-line-numbers` — reuse its algorithm only). Theme
  vars only. Pills/affordances reflow where applicable.
- Division-safe everywhere (line height 0/NaN → fallback; minimap with 0 rows → no NaN sizing; fold with
  start==end → no empty collapse).
- Generation-token the `codeIntel`/render so a superseded file can't paint stale rows (mirror `codeReq`).
- **`PreviewPane.test.ts` (~121-130)** asserts `.preview-text code span` with an `hljs-*` class — the
  per-row refactor must keep a matching span reachable (rows inside one `<code>` should still satisfy
  `code span`), or update that test **deliberately** (reviewed, not an accidental break). Keep the CPE-565
  wrap test + CPE-1090 outline tests green.
- Keep highlighting as one `hljs.highlight` call per file (async two-phase unchanged) — the split is a
  post-process on `.value`.

## Acceptance Criteria
- [ ] `splitHighlightedIntoLines` is pure + unit-tested: multi-line span, nested spans, empty input, trailing
      newline, span opening/closing mid-line — output rows re-concatenate to the original (minus the injected
      per-row wrappers) and every row is independently valid HTML (no unclosed spans).
- [ ] Opening a code file shows per-line rows with a line-number gutter; fold toggles appear on foldable lines
      and collapse/expand their range; indent guides reflect per-line depth; a minimap renders and its
      viewport indicator tracks scroll; clicking the minimap scrolls the pane.
- [ ] Unknown-lang/plain-text/no-fold files degrade gracefully (rows still render; no gutter errors; no
      minimap noise); edit mode + existing highlighting still work; select-all/copy still yields the whole file.
- [ ] `npm run check` clean; vitest green (incl. the new splitter tests + unchanged PreviewPane/outline tests,
      with any PreviewPane selector change explicitly justified); no new deps; no raw `@tauri-apps/api/core`.

## Work Log
2026-07-26 (workshift, GUI) — Filed by the Foreman as GUI slice #2 of the code-preview upgrade, on top of
CPE-1090 (outline strip) + the CPE-1089 `code_intel` command. Design de-risked by a research spike (filed to
the Library) that chose the one-blob-highlight + pure span-safe splitter + per-line-rows approach and flagged
the test/wrap/entity risks. Depends on CPE-1090 (same file) — dispatch after it merges.
