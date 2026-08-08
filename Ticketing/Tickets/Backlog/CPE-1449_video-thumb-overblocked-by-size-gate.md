---
id: CPE-1449
title: "Large video files get no frame thumbnail — the 128MiB preview size gate runs BEFORE the video-safe ffmpeg dispatch"
type: Bug
status: Backlog
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
