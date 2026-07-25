---
id: CPE-109
title: Preview/edit support for TAR archives files
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a first-class preview provider for TAR archives (.tar) in the right-side preview pane.
List entries read-only (no extraction). Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [x] .tar is matched by a dedicated preview provider, registered in the bundled provider registry
- [x] Viewer: List entries read-only (no extraction).
- [x] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [x] Backend support: Backend tar crate — lands green via CI (Rust builds/tests locally now too)
- [x] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [x] In-flight loads are cancelled when the selection changes
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend tar crate. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Resolution

Delivered .tar preview: the read_archive_entries backend command now dispatches by extension and lists TAR members via the pure-Rust `tar` crate (name/size/is_dir), reusing the shared archive preview pane (entry list + count). Rust unit test creates a tar and asserts the listing. Files: src-tauri/src/lib.rs (tar dep in Cargo.toml), src/lib/preview/provider.ts + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the archive-format batch (CPE-109/112/217). Rust: cargo test (39) + clippy clean; Frontend: npm run check clean, full vitest suite green (220).
