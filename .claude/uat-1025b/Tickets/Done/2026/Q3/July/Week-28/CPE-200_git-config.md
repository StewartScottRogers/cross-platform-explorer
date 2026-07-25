---
id: CPE-200
title: Preview/edit support for Git config/ignore/attributes files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 30m
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for Git config/ignore/attributes (.gitignore/.gitattributes) in the right-side preview pane. Highlighted git config family. Editable as source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .gitignore/.gitattributes is matched by a dedicated preview provider in the bundled registry
- [ ] Color-coded syntax highlighting via the highlight.js registry (register/enable the grammar; extends CPE-065). Escaped-monospace fallback if no grammar exists.
- [ ] Viewer: Highlighted git config family.
- [ ] Editing: Editable as source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [ ] In-flight loads cancelled on selection change
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: highlight.js properties/bash. Editing model: source.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered in the filename-matching batch (branch cpe-filenames): categoryOf now recognises well-known code files by name (Dockerfile, Makefile, .git* config, etc.) and highlight.ts resolves a language by full name (languageForName) so these preview highlighted and are editable. Tests in highlight.test.ts and filetypes.test.ts. check + suite green; build clean.
