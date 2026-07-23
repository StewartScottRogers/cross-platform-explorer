---
id: CPE-949
title: Batch metadata apply across a selection
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-725
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
Second headless slice of the media metadata studio (CPE-725), building on `media_meta_edit` (CPE-942).
`cpe_server::media_meta_batch`:
- `FileMeta { path, fields }` + `apply_batch(files, edits) -> Vec<FileEditResult>` — apply one shared set
  of edits to every file in a selection (each with its own current fields), preserving order.
- `summarize(results) -> BatchSummary { files, changed, unchanged, rejected }` — the run rollup.

The studio's "edit these 40 photos at once" core. Pure; the codec layer reads/writes.

## Acceptance Criteria
- [x] Same edits applied per file (update/add/reject per media_meta_edit); order preserved.
- [x] Summary counts changed/unchanged/rejected correctly; empty selection → zeros. 3 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Second CPE-725 slice: batch-apply edits across a selection + a run summary.
