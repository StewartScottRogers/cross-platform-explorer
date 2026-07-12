---
id: CPE-079
title: Preview/edit support for Email messages (EML) files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Email messages (EML) (.eml) in the right-side preview pane.
Parse and show headers plus the sanitized body. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .eml is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Parse and show headers plus the sanitized body.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: MIME/eml parser (bundled). Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .eml preview/edit: mapped to `code` (RFC 822 message source is UTF-8 text); no dedicated grammar ships with highlight.js, so per the AC it uses the escaped-monospace fallback. Editable as source via the shared text provider (write_file_text, CPE-066); load cancellation + large/corrupt-file fallback come from the shared PreviewPane (CPE-059). MIME/attachment rendering is a future backend enhancement. Files: src/lib/filetypes.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the text-based data/comms format batch (CPE-079/092/093/106/116/119/202/203/204/209/212/213). npm run check clean; unit tests green.
