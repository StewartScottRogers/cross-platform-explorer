---
id: CPE-634
title: "Cap file size for whole-file preview readers (avoid OOM)"
type: Bug
component: Backend
priority: low
status: Done
tags: ready
estimate: 20m
created: 2026-07-18
closed: 2026-07-18
---

## Summary
The binary/metadata preview readers (pe_info, midi_info, wasm_info, torrent_info, docx/odt/epub, and
read_image_data_url for PSD/TIFF) parse the WHOLE file, with no size guard — a maliciously huge file of
one of these types could exhaust memory. Add a single size guard at the `read_preview_info` dispatcher
(and `read_image_data_url`) so no reader slurps an absurd file. (`read_file_text`/`text_stats`/`hex_dump`
already bound their reads.)

## Acceptance Criteria
- [x] `read_preview_info` + `read_image_data_url` refuse files over `PREVIEW_INFO_MAX_BYTES` (128 MB).
- [x] Normal previews unaffected; a testable `ensure_previewable_size(path, cap)` helper is unit-tested.
- [x] cargo test + clippy clean.

## Resolution
Added `ensure_previewable_size` + `PREVIEW_INFO_MAX_BYTES` and called it at the two whole-file preview
entry points. Test `ensure_previewable_size_rejects_oversized_files`.

## Work Log
2026-07-18 (dayshift) — Found finishing the preview-reader audit (sibling of the hex_dump fix, CPE-633);
one guard at the dispatcher covers all the format parsers.
