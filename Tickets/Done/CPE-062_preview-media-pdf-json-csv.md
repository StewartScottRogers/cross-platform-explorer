---
id: CPE-062
title: Preview pane Phase 3a — audio, video, PDF, JSON and CSV providers
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Phase 3a of [[CPE-059]], extending the provider registry and `PreviewPane` with more file types that need
no new backend (media/PDF ride the already-enabled asset protocol; JSON/CSV reuse the existing
`read_file_text`). Fully frontend, so verified locally without a PR.

## Acceptance Criteria

- [x] Audio files preview in an `<audio controls>` (asset protocol src)
- [x] Video files preview in a `<video controls>`
- [x] PDF files preview in an `<iframe>` (native webview PDF viewer)
- [x] JSON files are pretty-printed (falling back to raw text on parse error)
- [x] CSV files render as a table (row-capped), via a tested `parseCsv`
- [x] JSON/CSV providers resolve before the generic text provider
- [x] Unit tests (provider selection, `parseCsv`) + `PreviewPane` jsdom tests; check + suite green

## Resolution

Added `audio`/`video`/`pdf`/`json`/`csv` provider kinds + registry entries (ordered before text),
a tested `src/lib/preview/csv.ts` parser, and the matching `PreviewPane` branches (media elements,
PDF iframe, JSON pretty-print, CSV table with a 200-row cap). `npm run check` 0 errors; suite 170
passed; `vite build` clean. No Rust needed → merged directly.

**Residual (human/GUI):** actual playback/rendering in the packaged app (folds into [[CPE-053]]).
Markdown rendering, syntax highlighting, archive listing and Office remain — see [[CPE-063]].
