---
id: CPE-020
title: File type icons and a Type column value
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Every file currently shows the same generic page glyph. Map common extensions to distinct icons and
human-readable type names ("PNG image", "Markdown file") for the Type column.

## Acceptance Criteria

- [x] Extension -> icon mapping for common types (image, doc, sheet, code, archive, audio, video, pdf)
- [x] Extension -> human-readable type name for the Type column
- [x] Folders show a folder icon; unknown types get a sensible default
- [x] Mapping is a pure module with unit tests

## Resolution

Added `src/lib/filetypes.ts`: `categoryOf()` maps ~50 extensions to 11 visual categories
(image/document/spreadsheet/presentation/pdf/code/archive/audio/video/text/unknown), and `typeName()`
produces Explorer's human-readable Type values ("PNG image", "Markdown file", "Visual Studio
Solution"), falling back to "QQQ File" for unknown extensions and plain "File" for extensionless ones.

Icons are inline SVG in a single `Icon.svelte` (no icon dependency), colour-coded like Explorer —
manila folders, green images, blue documents, red PDFs. Also covers every toolbar/sidebar glyph.
11 unit tests on the mapping.

## Work Log

2026-07-11 — Picked up. Wrote filetypes.ts as a pure module with 11 tests.
2026-07-11 — Used inline SVG rather than an icon package — no dependency, and full control over the Win11 colour coding. Closing as Done.

## Notes
