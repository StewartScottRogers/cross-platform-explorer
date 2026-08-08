---
id: CPE-1447
title: "thumbnails_stream skips the 128 MiB size gate the single-thumbnail command applies → OOM on a huge image file"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready, security]
epic: CPE-718
created: 2026-08-07
---
## Vector (found in the CPE-1437 resource-exhaustion sweep, 2026-08-07)
`src-tauri/src/lib.rs:~1203` `thumbnail` calls `ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)`
(`~:1205`), but the streaming batch `thumbnails_stream` (`~:1248`) → `run_thumb_batch` →
`thumbnail_cached`/`make_thumbnail_png` → `thumb_source::decode_thumb_image` does `std::fs::read(path)`
with **no gate**. The grid uses the streaming path, so the gate is effectively bypassed for the common case.

## Concrete pathological input
A folder containing a 6 GB file with an image extension (e.g. `huge.png`, even all zeros). Scrolling it
into view → `fs::read` slurps 6 GB → OOM. Lower amplification than CPE-1445/1446 (≈1:1 — attacker needs a
genuinely large file), but a real crash from merely browsing, and the guard already exists 40 lines away.
This is an inconsistency, not a design question.

## Fix direction
Apply `ensure_previewable_size` inside the `run_thumb_batch` compute closure (or per-request before
decode), mirroring the single `thumbnail` command. The gate is a Tauri-layer helper today, so place it
where the per-file read happens. A skipped oversize file falls back to the type icon (same UX as any decode
failure). Add a test that an oversize file is skipped by the streaming path.

## Effort / blast radius
XS / one closure in lib.rs — disjoint from the SVG (thumb_svg.rs) and doc_text.rs work, parallel-safe.
