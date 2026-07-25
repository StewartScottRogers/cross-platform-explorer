---
id: CPE-065
title: Preview pane — syntax-highlighted code and rendered (sanitized) markdown
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Two rich-text preview improvements from [[CPE-063]]:
- **Syntax highlighting** for code files (currently shown as raw text).
- **Markdown rendering** for `.md` (currently shown as raw text), sanitized so an untrusted file can't
  inject script.

Decisions made autonomously (per "do everything"): `highlight.js` (core + a curated language set, so the
bundle only carries what we register) for code; `marked` + `DOMPurify` for markdown (parse then
sanitize). All bundled — no external/CDN requests, CSP-safe.

## Acceptance Criteria

- [ ] Code files render highlighted (hljs token spans) for known languages; unknown types fall back to
      safely-escaped plain text
- [ ] `.md` renders to HTML (headings, bold, lists, links)
- [ ] Markdown HTML is sanitized: `<script>`, `on*=` handlers and `javascript:` URLs are stripped
- [ ] Injection is safe: hljs escapes code; DOMPurify sanitizes markdown (both then use `{@html}`)
- [ ] Pure helpers (`highlightCode`, `renderMarkdown`) unit-tested; `PreviewPane` jsdom tests
- [ ] `npm run check` clean; JS suite green; `vite build` clean

## Resolution

Added `src/lib/preview/highlight.ts` (highlight.js core + 10 curated languages; `highlightCode` returns
hljs spans for known languages, escaped plain text otherwise) and `src/lib/preview/markdown.ts`
(`renderMarkdown` = marked → DOMPurify). `PreviewPane` now renders markdown via `{@html renderMarkdown}`
and code via `{@html highlightCode(text, ext)}` in `<pre><code>`; imported the github hljs theme in
`main.ts`. 9 unit tests (highlight + markdown incl. script/handler stripping) + 3 `PreviewPane` jsdom
tests. `npm run check` 0 errors; suite 184 passed; `vite build` clean (bundle +~50 KB gzip — accepted).
Merged to `main`.

**Residual (human/GUI):** on-screen appearance of highlighted code / rendered markdown (folds into
[[CPE-053]]).

## Work Log

2026-07-11 — Nightshift "do everything": chose highlight.js (curated langs) + marked/DOMPurify without asking, per the standing autonomy instruction. Both render via `{@html}` with escaped/sanitized input.

## Notes

Closes the code-highlight + markdown items of [[CPE-063]]. Remaining CPE-063 items (Office, list-view
thumbnails) are documented decisions there.
