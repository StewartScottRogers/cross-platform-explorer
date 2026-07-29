---
id: CPE-220
title: Lazy-load preview grammars and heavy renderers (bundle + async provider pattern)
type: Task
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Preparatory infrastructure for the remaining preview tickets. The ~70 highlight.js grammars and the
markdown renderer (marked + DOMPurify) were bundled eagerly, ballooning the initial JS to ~161 KB gzip.
Convert them to lazy dynamic imports so they code-split into on-demand chunks, and establish the
async-render pattern that heavier future providers (PDF, 3D, DOCX, mermaid, etc.) will follow.

## What changed

- `highlight.ts`: start from highlight.js/lib/core (no languages); a `LOADERS` map dynamically imports
  each grammar; `ensureLanguage`/`ensureLanguageForName` register on demand; `highlightForFile` is
  sync-escape-until-ready. `languageForExt`/`languageForName` resolve by loader existence.
- `markdown.ts`: `renderMarkdown` is now async, dynamically importing marked + DOMPurify.
- `PreviewPane.svelte`: renders code and markdown asynchronously (`renderCode`/`renderMd`) — escaped
  text shows instantly, highlighted/rendered HTML swaps in once its chunk loads; stale-request guards.

## Result

- Initial `index` chunk: **~161 KB → ~55 KB gzip**. Grammars/marked/DOMPurify are now separate chunks
  loaded only when a matching file is previewed.
- `npm run check` clean; full suite green (200 tests); `vite build` clean (per-grammar chunks emitted).

## Notes

Unblocks adding heavy providers without bloating startup — each new render library gets its own lazy
chunk via the same pattern. Relates to [[CPE-059]] [[CPE-065]].
