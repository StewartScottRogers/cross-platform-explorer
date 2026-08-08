---
id: CPE-1449
title: "Large video files get no frame thumbnail — the 128MiB preview size gate runs BEFORE the video-safe ffmpeg dispatch"
type: Bug
status: Done
priority: Medium
component: Backend
tags: [ready]
epic: CPE-720
created: 2026-08-07
---
## Observation (from CPE-1447 UAT of PR #711)
`thumb_source::decode_thumb_image` dispatches video extensions EARLY, straight to ffmpeg against the path —
by design, so a large video is never slurped into memory. But BOTH the single `thumbnail` command
(`src-tauri/src/lib.rs:~1212`, pre-existing) AND the streaming `thumb_compute` (added in CPE-1447) call
`ensure_previewable_size(path, PREVIEW_INFO_MAX_BYTES=128MiB)` BEFORE that video-safe dispatch is reached.
Videos routinely exceed 128MiB, so a legitimately large `.mp4`/`.mkv`/etc. is refused with "too large" and
shows a generic type icon instead of a frame thumbnail — in both the single command and the grid.

This is PRE-EXISTING (CPE-1447 correctly mirrored the single command's behavior; it did not introduce or
worsen it) — but it's a real UX gap: video thumbnails silently never appear for large files.

## Fix direction
Special-case video in the size gate: for a video extension (the set `decode_thumb_image` routes to ffmpeg),
SKIP the pre-decode `ensure_previewable_size` check (ffmpeg streams frames, it does not read the whole file
into memory), or apply a much larger / no cap on the ffmpeg branch only. Apply the fix to BOTH the single
`thumbnail` command and `thumb_compute` so they stay consistent. Keep the 128MiB gate for image/raster
extensions (that path DOES `fs::read` the whole file — CPE-1447's actual fix). Add a test that a large
(sparse) `.mp4` is NOT rejected by the gate while a large `.png` still is.

## Notes
Serialize AFTER CPE-1447 merges (same `thumb_compute`/lib.rs code). Verify which extensions ffmpeg handles
by reading `decode_thumb_image` so the skip-set matches exactly. Epic CPE-720 (audio/video player pane) /
CPE-718 (thumbnail pipeline).

## Resolution (2026-08-07) — subsumed by CPE-1447 PR #711

An independent reviewer caught exactly this bug — as a regression, not just the pre-existing instance —
in CPE-1447's first attempt on PR #711 (attempt 1 had moved the size gate into a `thumb_compute` helper
in `src-tauri/src/lib.rs`, called unconditionally before dispatch, same class of bug this ticket
describes against the single `thumbnail` command). The fix chosen for CPE-1447's attempt 2 is exactly
this ticket's preferred fix direction: the size gate now lives inside
`crates/server/src/thumb_source.rs::decode_thumb_image`, placed AFTER the video early-return (which
dispatches to `thumb_video::extract_frame`/ffmpeg and never reads the file into memory) and BEFORE the
`fs::read` the raster/PSD/SVG/font/PDF branches share. Since both the single `thumbnail` command and the
streaming `thumbnails_stream` batch funnel through this one `decode_thumb_image` call site (via
`thumbnail_cached`/`make_thumbnail_png`), both entry points are fixed by construction — no separate
special-casing needed in `lib.rs` at all; the old `ensure_previewable_size`/`PREVIEW_INFO_MAX_BYTES`
calls at the thumbnail command and in `thumb_compute` were removed outright (the gate isn't duplicated
anywhere anymore).

Regression pin: `crates/server/src/thumb_source.rs::decode_thumb_image_does_not_size_gate_a_video_extension`
(`#[cfg(feature = "video-thumb")]`) — a sparse `.mp4` far over the 128 MiB cap must not be rejected with
the size-gate's "too large" message.

No separate work needed — this ticket is fully covered by CPE-1447's Work Log (same PR #711, same
branch `cpe-1447-thumbnails-stream-size-gate`). Closed as subsumed, not implemented independently.
