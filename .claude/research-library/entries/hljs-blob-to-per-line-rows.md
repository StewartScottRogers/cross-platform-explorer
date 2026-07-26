---
title: "How to render highlight.js output as per-line rows (for a line gutter, fold gutter, indent guides, minimap)?"
date: 2026-07-26
tags: [highlight-js, code-preview, per-line, fold-gutter, indent-guides, minimap, previewpane, cpe-724, cpe-1091]
status: current
---

## Question
The code preview injects one highlight.js HTML blob (`<pre><code>{@html codeHtml}</code></pre>`). To add a
line-number gutter, a **fold** gutter, per-line indent guides, and a synchronized minimap, how do we get
per-line structure — split the blob, re-highlight per line, a tokenizer, or a CSS overlay?

## Finding (short)
**Hybrid = keep one-blob highlighting + a lean span-safe splitter into per-line rows.** Fold is the forcing
function: an overlay can only *draw* a triangle, it can't hide a run of lines out of a single blob — you must
split at line boundaries. So: `hljs.highlight(code,{language}).value` stays one call per file (async
two-phase render unchanged), then a pure `splitHighlightedIntoLines(html): string[]` post-processes it into
one span-safe HTML fragment per source line; PreviewPane renders `{#each codeLines as line,i}` rows carrying
`data-line={i+1}`, and the gutter/fold/indent attach to the matching row (no line-height math for those).
Minimap stays a separate small render off `MinimapRow[]`, scroll-synced like CPE-1090's breadcrumb.

## The render seam today
- `src/lib/preview/highlight.ts`: `highlight.js/lib/core` (line 11), lazy grammar loaders (`ensureLanguage`,
  166-170). `highlightForFile(code,name)` (189-195) → single call `hljs.highlight(code,{language:lang}).value`
  (192) = one flat HTML string; `escapeHtml` (180-182) is only the pre-grammar fallback. Multi-line
  constructs (block comments/template literals) produce a `<span>` whose content has raw `\n` — **span not
  closed at line boundaries** (the crux).
- `PreviewPane.svelte:178-183` `renderCode` = two-phase (escaped immediately, re-highlight after grammar
  loads, `codeReq` generation token). Injected at `:459` `<pre class="preview-text" class:nowrap>...`.
  `.preview-text` (507-516) is `pre-wrap`, switches to `pre; overflow-x:auto` when `wrapLines` off (CPE-565).
  No per-line DOM today.

## Approaches weighed
- **A. Close/reopen hljs spans at each `\n`** (the `highlightjs-line-numbers.js` algorithm): one linear scan
  over already-highlighted markup, track open-span stack, close-all at `\n`, reopen on next line. Robust,
  dependency-free (reuse the algorithm, don't add the abandoned lib). **← chosen.**
- **B. Per-line highlight via hljs continuation/internal state** — relies on undocumented `core` internals,
  fragile across version bumps. Rejected.
- **C. Tokenizer-per-line (CodeMirror/Monaco/Shiki model)** — native per-line, but an architecture change /
  heavier dep than the lean app wants. Rejected.
- **D. CSS-only overlay (extend CPE-1090's line-height math)** — great for line numbers/minimap viewport, but
  **cannot fold** (no per-line click target, can't collapse a run out of one blob) and indent depth varies
  per line so a single gradient can't do guides. Not sufficient alone; its math is still right for the
  minimap viewport indicator.

## Backend data already present (confirmed in bindings.gen.ts)
`CodeIntel = { outline: Symbol[]; folds: FoldRange[]; indent: number[]; minimap: MinimapRow[] }` (:1709).
`FoldRange {start_line,end_line,kind}` 1-based inclusive, kind ∈ block|suite|section (:1872-1888).
`indent: number[]` = one entry **per source line**. `MinimapRow {fill:0-255, indent}` = bucketed per-row
(:1954-1962). `codeIntel(text,lang,tabWidth,minimapBuckets)` (:208). So indent-per-line + minimap are
precomputed server-side; frontend only positions/paints — favours indexing into a per-line row array.

## Recommended seam
New pure `splitHighlightedIntoLines(html: string): string[]` colocated with highlight.ts (string→string[], no
DOM, unit-test like outline.ts). `renderCode` computes `codeHtml` as today then `codeLines =
splitHighlightedIntoLines(codeHtml)` for the code path only (json/md/text unaffected). Replace the `:459`
blob with `{#each codeLines as line,i}` rows inside ONE `<pre>`/`<code>` (rows are `<span>`/`<div>`, not
multiple `<code>`, to keep select-all/copy sane), each `data-line={i+1}`; gutter (numbers + fold triangles
from FoldRange) + indent guides (indent[i]) attach per row; minimap separate, scroll-synced.

## Risks to encode in the ticket
1. `PreviewPane.test.ts:121-130` asserts `.preview-text code span` with an `hljs-*` class — the refactor must
   keep a matching span (likely still matches since `code span` isn't depth-anchored) or update the test
   deliberately (reviewed, not accidental).
2. **Wrap toggle**: with wrap on, one source line spans multiple visual rows → uniform line-height overlays
   break. A `data-line` per-source-line wrapper still works (row just grows), but the feature should compose
   with the `nowrap` branch — force/hide the wrap toggle when folds/indent-guides are shown.
3. Split must scan the **already-escaped** HTML (hljs emits `&lt;`/`&gt;`/`&amp;`) — scan only literal
   `<span`/`</span>`/`\n` tokens; hljs never emits raw angle brackets outside escaped forms.
