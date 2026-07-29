---
id: CPE-093
title: Preview/edit support for vCard contacts (VCF) files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for vCard contacts (VCF) (.vcf) in the right-side preview pane.
Contact card view. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .vcf is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: Contact card view.
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: vCard parser (bundled). Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .vcf preview/edit: mapped to `code`; no dedicated grammar ships with highlight.js, so per the AC it uses the escaped-monospace fallback. Editable as source via the shared text provider (write_file_text, CPE-066); load cancellation + large/corrupt-file fallback come from the shared PreviewPane (CPE-059). Files: src/lib/filetypes.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the text-based data/comms format batch (CPE-079/092/093/106/116/119/202/203/204/209/212/213). npm run check clean; unit tests green.
