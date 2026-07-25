---
id: CPE-151
title: Preview/edit support for Nim files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for Nim (.nim) in the right-side preview pane. Highlighted Nim source. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .nim is matched by a dedicated preview provider in the bundled registry
- [ ] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [ ] Viewer: Highlighted Nim source.
- [ ] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js grammar: nim. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered in the individual-grammars batch (branch cpe-langs-extra): registered the highlight.js grammar
and mapped the extension(s) in highlight.ts, and added the extension(s) as "code" in filetypes.ts — so
this type previews with color-coded syntax highlighting and is editable as source (text provider +
write_file_text). Representative coverage in highlight.test.ts. npm run check clean; suite green; build
clean.
