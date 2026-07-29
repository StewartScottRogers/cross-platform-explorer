---
id: CPE-217
title: Preview/edit support for JAR / APK archives files
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 1h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for JAR / APK archives (.jar/.apk) in the right-side preview pane. Entry list. Read-only viewer.

## Acceptance Criteria

- [x] .jar/.apk is matched by a dedicated preview provider in the bundled registry
- [x] Viewer: Entry list.
- [x] Editing: Read-only viewer.
- [x] Backend support: reuse read_archive_entries (zip) — verified locally (cargo) and green via CI.
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: reuse read_archive_entries (zip). Editing model: none.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered .jar/.apk/.war/.ear/.ipa/.xpi preview: these are all ZIP containers, so the archive provider now matches them and the existing zip-reading backend lists their entries unchanged. Frontend-only for the reader; added the extensions to filetypes.ts (archive category + friendly names) and the archive provider set. Files: src/lib/preview/provider.ts, src/lib/filetypes.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the archive-format batch (CPE-109/112/217). Rust: cargo test (39) + clippy clean; Frontend: npm run check clean, full vitest suite green (220).
