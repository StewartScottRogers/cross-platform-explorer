---
id: CPE-1090
title: "Code preview: symbol outline strip + breadcrumb + jump-to-symbol"
type: feature
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-26
epic: CPE-724
---

## Summary
Child of CPE-724 (Code intelligence preview), GUI slice #1 of 2. The backend `code_intel` command
(CPE-1089, merged) now returns `{ outline, folds, indent, minimap }` for a file's text, but **nothing in the
GUI calls it**. This ticket wires the **outline** into `PreviewPane.svelte`: a clickable **symbol outline
strip**, a **breadcrumb** that tracks the top-visible symbol as you scroll, and **jump-to-symbol** (click a
symbol → scroll the preview to that line). Keep the existing single-blob `<pre>` render **unchanged** — this
slice is purely additive UI over `outline`; the minimap / fold gutter / indent guides that need a per-line
refactor are the **next** ticket (CPE-1091), so DO NOT refactor the `<pre>` here.

## Context (verified in tree)
- `src/lib/components/PreviewPane.svelte` (618 lines). Code files render at **line 459** as one blob:
  `<pre class="preview-text" class:nowrap={!wrapLines} bind:this={textContentEl}><code>{@html codeHtml}</code></pre>`
  (the `{:else}` branch — i.e. `provider.kind` is the code/"text-ish highlighted" case, not csv/json/markdown).
- `text` (the file's raw string) and `name` (filename) are already in scope; `codeHtml` is computed at
  ~lines 166–182 via `highlightForFile(src, name)`. `textContentEl` is the bound `<pre>`.
- New binding (already generated, do NOT regenerate): `codeIntel(text, lang, tabWidth?, minimapBuckets?)`
  from `src/lib/bindings.gen.ts`, returning `CodeIntel { outline: Symbol[]; folds; indent; minimap }` where
  `Symbol = { name: string; kind: SymbolKind; line: number }` (line is **1-based**).
- **IMPORTANT — import `codeIntel` via the busy-cursor wrapper**, not `@tauri-apps/api/core`. Check how other
  bindings are invoked in this file / repo; `bindings.gen.ts` already routes through `./invoke`
  (busy-cursor) per the drift-guard, so calling the generated `codeIntel` binding is correct. Do not add a
  raw invoke.

## Design (buildable)
1. **Fetch the outline when a code file's text is ready.** In the reactive block that already reacts to
   `entry && textState === "idle"` for the code case (near lines 171–182), after `text` is loaded, call
   `codeIntel(text, lang)` and store `let outline: Symbol[] = []`. Guard: only for the code branch (not
   csv/json/markdown/hex/etc.); clear `outline = []` when the entry changes or text is empty. Derive `lang`
   from the filename extension — reuse whatever the highlight module exposes (e.g. a `languageForName(name)`
   / `LANG_BY_EXT` in `src/lib/preview/highlight.ts`); if no known lang, pass `""` (the backend returns an
   empty outline — fine). Use a generation token (like the existing `codeReq` pattern at ~line 178) so a
   slow `codeIntel` for a superseded file can't overwrite a newer file's outline.
2. **Outline strip** — a thin horizontal, **reflowing** bar rendered **above** the `<pre>` (only when
   `outline.length > 0`). Each symbol is a pill/chip button showing a small kind glyph + `name`. The row of
   pills MUST reflow per the tick-tacks rule: `display:flex; flex-wrap:wrap; gap` on the container; each pill
   `white-space:nowrap; flex:0 0 auto` with `max-width` + ellipsis (symbol names can be long). Pills use
   **theme variables only** (`var(--text)`, `var(--surface)`, `var(--border)`, `var(--accent)`) — no
   hard-coded colours, identical light/dark. Map `SymbolKind` → a leading `Icon` glyph (reuse the existing
   `Icon` component / glyph set; pick sensible glyphs — function, class/struct, method, const, etc.).
3. **Jump-to-symbol** — clicking a pill scrolls the preview so that symbol's line is at (or near) the top.
   Because the render is a single blob (no per-line DOM), compute the offset from the uniform line height:
   measure the `<pre>`'s line height once (e.g. `parseFloat(getComputedStyle(el).lineHeight)`, fall back to
   `fontSize * 1.4` if `lineHeight` is `"normal"`), then set the scroll container's `scrollTop = (line - 1) *
   lineHeight`. Determine the actual scroll container (the `<pre class="preview-text">` itself if it scrolls,
   else its scrolling ancestor) — verify by inspecting the CSS; the pre likely has `overflow:auto`. Clamp to
   `[0, scrollHeight]`. Do the math with plain numbers; guard `line >= 1`.
4. **Breadcrumb** — a single line above/within the strip showing the **enclosing symbol of the top-visible
   line**: on the preview's `scroll` event, compute `topLine = round(scrollTop / lineHeight) + 1`, then pick
   the last `outline` symbol whose `line <= topLine` (outline is in source order by line). Show its kind
   glyph + name (or empty when above the first symbol). Debounce/throttle is optional but keep the handler
   cheap (a linear scan of `outline` is fine — outline is small; if you want, precompute nothing). Removing
   the listener on destroy / entry change is required (no leaks).
5. **No layout regression** — the strip + breadcrumb sit in the preview header area above the `<pre>`; the
   `<pre>` keeps its own scroll. Editing mode (`startEdit` → `<textarea>`) and the wrap toggle must still
   work; hide the outline strip while editing (outline is for the read view).

## ⚠ Notes / guardrails
- **Do NOT refactor the `<pre>` into per-line rows** — that's CPE-1091. Keep line 459's blob render.
- No new deps. No backend changes (command already merged). Theme-variable colours only (MENUS/tick-tacks
  conventions). Pills reflow; text never wraps inside a pill.
- Division-safe: if measured `lineHeight` is `0`/NaN, fall back to a sane constant (e.g. 18) — never divide
  by zero, never `scrollTop = NaN`.
- Generation-token the async `codeIntel` call (supersede on entry change) — mirror the existing `codeReq`.
- Keep it accessible: pills are `<button>`s with `aria-label`; the strip is keyboard-reachable.

## Acceptance Criteria
- [ ] Opening a code file (e.g. a `.rs`/`.ts`/`.py`) in the preview shows an outline strip of its symbols;
      a file with no symbols (or unknown lang) shows **no** strip (no empty bar, no error).
- [ ] Clicking a symbol scrolls the preview to that symbol's line (top-aligned, within a line or two).
- [ ] The breadcrumb updates as you scroll to show the enclosing symbol of the top-visible line.
- [ ] Pills reflow onto multiple rows and never overflow their background; colours come from theme vars
      (identical light/dark); switching to edit mode hides the strip and editing still works.
- [ ] `npm run check` clean; `npm test` (vitest) green; no new deps; no `@tauri-apps/api/core` raw invoke
      added (uses the generated `codeIntel` binding through the busy-cursor wrapper).
- [ ] If any pure helper is added (e.g. `enclosingSymbol(outline, topLine)` / `lineToScrollTop`), it lives
      in a small testable module with a unit test (division-by-zero + empty-outline + boundary cases).

## Work Log
2026-07-26 (sprint, GUI) — Filed by the Foreman as GUI slice #1 of the code-preview upgrade, on top of the
merged `code_intel` command (CPE-1089). Scoped to the outline (breadcrumb + jump) so it stays blob-compatible
and conflict-free with CPE-1091 (minimap/folds/indent, which does the per-line refactor and runs next).
