---
id: CPE-067
title: Preview providers declare whether their file type is editable
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 30m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Some files are meant to be edited (text, code, markdown, JSON, CSV); others are not (images, audio,
video, PDF, archives). Add an `editable` flag to the `PreviewProvider` model so the UI can show an
"Edit" affordance only where editing makes sense.

## Acceptance Criteria

- [ ] `PreviewProvider` gains `editable: boolean`
- [ ] Editable: text, markdown, json, csv. Not editable: image, audio, video, pdf, archive, none
- [ ] A helper (e.g. `pickProvider(entry).editable`) exposes it to the pane
- [ ] Unit tests assert editability per kind
- [ ] `npm run check` clean; suite green

## Resolution

Added `editable: boolean` to `PreviewProvider`: true for text/markdown/json/csv, false for
image/audio/video/pdf/archive and the metadata fallback. Exposed via `pickProvider(entry).editable`.
Unit test asserts editability per kind. `npm run check` clean; suite green.

## Work Log

2026-07-11 — Part of the content-editor set. Editability is a property of the provider so it rides the existing plugin/registry architecture. Relates to [[CPE-066]] and [[CPE-068]].

## Notes

Editing JSON/CSV/markdown edits the RAW text (not a structured editor) — consistent with how the viewer
loads them.
