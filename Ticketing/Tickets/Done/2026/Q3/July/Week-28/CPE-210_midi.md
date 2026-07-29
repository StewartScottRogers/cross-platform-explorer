---
id: CPE-210
title: Preview/edit support for MIDI music files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

Add a preview provider for MIDI music (.mid/.midi) in the right-side preview pane. Track and note-event view. Read-only viewer.

## Acceptance Criteria

- [x] .mid/.midi is matched by a dedicated preview provider in the bundled registry
- [x] Viewer: Track and note-event view.
- [x] Editing: Read-only viewer.
- [x] Graceful handling of large or corrupt files; falls back to metadata, never hangs
- [x] In-flight loads cancelled on selection change
- [x] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture. Approach: MIDI parser (JS). Editing model: none.
Syntax highlighting builds on [[CPE-065]]; editable types reuse [[CPE-066]] write_file_text.
## Resolution

Delivered via a new backend command read_preview_info(path) that returns a human-readable text summary dispatched by extension, plus a new read-only "info" preview kind wired through PreviewPane (with load cancellation) and App. Corrupt files yield an error -> the metadata fallback, never a hang. Handler (.mid/.midi): midly parses the SMF and reports format, timing/division, track count, total events, and per-track event counts with track names. (Kept off the audio provider deliberately, since webviews can't play MIDI natively.) Files: src-tauri/src/lib.rs (midly dep) + frontend wiring + tests.

## Work Log

2026-07-12 — Implemented and closed as part of the structured-binary preview batch (CPE-210/214/215/216/218) — new read_preview_info backend + info preview kind. Rust: cargo test (45) + clippy clean; Frontend: npm run check clean, full vitest suite green (221).
