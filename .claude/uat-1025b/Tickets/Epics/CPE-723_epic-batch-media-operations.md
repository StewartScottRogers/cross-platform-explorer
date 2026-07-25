---
id: CPE-723
title: "EPIC: Batch media operations"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Bulk, non-destructive-by-default transforms over a selection of images/media: resize, convert format,
rotate/flip, strip or preserve metadata, compress/optimize, watermark, and pattern-based rename — with a
live before/after preview and a progress panel.

## Why
Complements the read-only preview stack with light editing at scale. "Resize these 200 photos and convert
to WebP" is a common chore that currently forces users out to another app.

## Rough scope (areas, not child tickets)
- A Rust transform worker (reuse transfer-manager progress + streaming conventions).
- A batch-op dialog: pick operations, parameters, output naming/location, live preview.
- Non-destructive default (write copies) with an explicit in-place option.
- Metadata strip/preserve controls; watermarking.

## Open questions (resolve at activation)
- Image/media library choices and format coverage vs. build size.
- Output policy defaults (copies vs. in-place) and conflict handling.
- Overlap with the media-metadata studio ([[CPE-725]]) for strip/preserve.

## Definition of Done
- Users can run resize/convert/rotate/compress/watermark over a multi-selection with live preview.
- Transforms run through a progress panel and default to non-destructive output.
- Pattern-based rename integrates; no regression to single-file preview/edit.

## Work Log
2026-07-23 (dayshift) — **Activated.** First slice: **CPE-940** — `batch_media::plan` / `validate`: the pure
planner turning media ops + a selection into collision-safe, non-destructive output paths + summaries.
Remaining: the actual transforms (resize/convert/rotate/…), the before/after preview, and the progress panel.
