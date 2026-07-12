---
id: CPE-170
title: Preview/edit support for LLVM IR files
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 1h
created: 2026-07-11
closed:
---

## Summary

Add a preview provider for LLVM IR (.ll) in the right-side preview pane. Highlighted LLVM IR. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .ll is matched by a dedicated preview provider in the bundled registry
- [ ] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [ ] Viewer: Highlighted LLVM IR.
- [ ] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js grammar: llvm. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.