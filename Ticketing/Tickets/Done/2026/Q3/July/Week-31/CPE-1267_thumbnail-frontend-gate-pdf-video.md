---
id: CPE-1267
title: "PDF/video thumbnails never render in the grid — frontend hasThumbnail() gate wasn't updated"
type: bug
component: frontend
priority: high
status: Done
tags: ready
created: 2026-08-02
epic: CPE-718
---

## Problem (found by user eyeball on installed v0.57.39)
PDF + video files showed generic icons in the Large-icons/Gallery grid even though pdfium.dll + ffmpeg.exe
were bundled and the backend `decode_thumb_image` handles pdf/video. Root cause: the FRONTEND gate
`hasThumbnail()` in `src/lib/fileTypes.ts` uses `THUMBNAIL_EXTRA_EXTS`, which was updated for CPE-1236
(psd/svg/font) but NOT for CPE-1256 (pdf) or CPE-1257 (video). So `FileList.svelte:626` never rendered a
`<ThumbnailImage>` for those extensions → the grid never REQUESTED the backend thumbnail → generic icon.
Classic end-to-end wiring gap: backend decoders built + reviewed, but the frontend allowlist drifted out of
sync with `thumb_source`'s dispatch. The existing `filetypes.test.ts` even asserted `hasThumbnail("clip.mp4")
=== false`, encoding the bug.

## Fix
- Added `pdf` + the 10 video extensions (mirrors `cpe_server::thumb_video::VIDEO_EXTENSIONS`:
  mp4/mov/mkv/webm/avi/m4v/mpg/mpeg/wmv/flv) to `THUMBNAIL_EXTRA_EXTS`.
- Updated `filetypes.test.ts` to assert pdf + all video exts are thumbnailable (regression guard) and removed
  the stale `clip.mp4 === false` assertion.

## Verify
- `npx vitest run src/lib/filetypes.test.ts` → 38/38 pass. No downstream secondary gate (ThumbnailImage /
  thumbnails_stream don't re-filter by type), so this single gate fix completes the end-to-end path.
- End-to-end (installed build) verified by user eyeball on the follow-up release.

## Follow-up
Consider a drift guard so the frontend `THUMBNAIL_EXTRA_EXTS` can't fall out of sync with the backend
dispatch again (e.g. export the backend's supported-ext list to TS and assert equality).
