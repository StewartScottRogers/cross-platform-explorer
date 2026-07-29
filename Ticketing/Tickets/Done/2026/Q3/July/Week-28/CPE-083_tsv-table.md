---
id: CPE-083
title: Preview/edit support for Tab-separated values (TSV) files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for Tab-separated values (TSV) (.tsv) in the right-side preview pane.
Table view; edit cells (raw text). Editable (raw or structured), saved via the write_file_text command (CPE-066).

## Acceptance Criteria

- [ ] .tsv is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: Table view; edit cells (raw text).
- [ ] Editing: Editable (raw or structured), saved via the write_file_text command (CPE-066).
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Reuse the CSV parser with a tab delimiter. Editing model: structured. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Added a dedicated TSV provider that renders a table (CSV parser generalised with a tab delimiter) and is editable as source. Tests in provider/PreviewPane/csv. check + suite green.
