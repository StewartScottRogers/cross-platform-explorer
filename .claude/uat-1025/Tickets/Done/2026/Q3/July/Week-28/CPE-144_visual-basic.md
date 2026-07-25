---
id: CPE-144
title: Preview/edit support for Visual Basic .NET files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for Visual Basic .NET (.vb) in the right-side preview pane. Highlighted VB.NET source. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .vb is matched by a dedicated preview provider in the bundled registry
- [ ] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [ ] Viewer: Highlighted VB.NET source.
- [ ] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js grammar: vbnet. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered in the common-languages batch (branch cpe-langs-common): switched the highlighter to
highlight.js/lib/common and expanded the extension→language map (highlight.ts) plus the file-type
tables (filetypes.ts), so this type previews with color-coded syntax highlighting and is editable as
source (text provider + write_file_text). Representative coverage in highlight.test.ts and
filetypes.test.ts. npm run check clean; suite green; vite build clean.
