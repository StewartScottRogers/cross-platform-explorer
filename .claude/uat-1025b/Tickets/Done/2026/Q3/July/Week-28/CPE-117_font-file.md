---
id: CPE-117
title: Preview/edit support for Fonts (TTF/OTF/WOFF) files
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Fonts (TTF/OTF/WOFF) (.ttf/.otf/.woff) in the right-side preview pane.
Specimen text and a glyph grid via @font-face. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .ttf/.otf/.woff is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Specimen text and a glyph grid via @font-face.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: FontFace API. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .ttf/.otf/.woff/.woff2 preview: a new "font" preview kind loads the font file through the webview FontFace API (via convertFileSrc) and renders a live specimen — the pangram at five sizes in the actual typeface — with graceful states (no backend needed; degrades cleanly where FontFace is unavailable, e.g. jsdom). Files: src/lib/preview/provider.ts, PreviewPane.svelte + provider tests.

## Work Log

2026-07-12 — Implemented and closed as part of the ISO + font batch (CPE-113/117). Rust: cargo test (54) + clippy clean; Frontend: npm run check clean, full vitest suite green (226).
