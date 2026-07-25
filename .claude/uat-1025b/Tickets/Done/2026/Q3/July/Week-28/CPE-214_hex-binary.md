---
id: CPE-214
title: Preview/edit support for Generic binary (hex viewer) files
type: Feature
status: Done
priority: Low
component: Multiple
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for Generic binary (hex viewer) (.bin/.dat) in the right-side preview pane. Hex + ASCII dump. Read-only viewer.

## Acceptance Criteria

- [x] .bin/.dat is matched by a dedicated preview provider in the bundled registry
- [x] Viewer: Hex + ASCII dump.
- [x] Editing: Read-only viewer.
- [x] Backend support: backend byte read — verified locally (cargo) and green via CI.
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: backend byte read. Editing model: none.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered via a new backend command read_preview_info(path) that returns a human-readable text summary dispatched by extension, plus a new read-only "info" preview kind wired through PreviewPane (with load cancellation) and App. Corrupt files yield an error -> the metadata fallback, never a hang. Handler: a classic hex+ASCII dump of the first 64 KB (offset column, hex bytes, printable-ASCII gutter). Matches generic binary (.bin/.dat) and is the default for any binary routed to the info provider. Rust unit test asserts the offset/hex/ASCII formatting. Files: src-tauri/src/lib.rs, src/lib/preview/provider.ts, src/lib/components/PreviewPane.svelte, src/App.svelte + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the structured-binary preview batch (CPE-210/214/215/216/218) — new read_preview_info backend + info preview kind. Rust: cargo test (45) + clippy clean; Frontend: npm run check clean, full vitest suite green (221).
