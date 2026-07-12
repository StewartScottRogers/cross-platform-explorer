---
id: CPE-073
title: Preview/edit support for LaTeX source (TeX) files
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add a first-class preview provider for LaTeX source (TeX) (.tex) in the right-side preview pane.
Edit the source; optionally render math with KaTeX. Editable as raw source text, saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .tex is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Edit the source; optionally render math with KaTeX.
- [ ] Editing: Editable as raw source text, saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: KaTeX (optional, bundled). Editing model: source. Editable types reuse [[CPE-066]] write_file_text.
