---
id: CPE-1257
title: "Video representative-frame thumbnail extractor (bundled ffmpeg shell-out, feature-gated)"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-02
epic: CPE-718
---

## Summary
Slice 2 of CPE-1238. Add a **video representative-frame → thumbnail** extractor behind a new off-by-default
`video-thumb` feature by **shelling out to a bundled `ffmpeg` executable** (no linking — keeps LGPL/GPL out
of the signed binary). See research-library `thumbnail-native-deps-pdf-video-2026-08-02.md`.

## Build
- New `crates/server/src/thumb_video.rs`: `pub fn extract_frame(path: &Path, max_edge: u32) -> Result<DynamicImage, String>`.
  Shell `ffmpeg -ss <~10% of duration or a fixed early offset> -i <path> -frames:v 1 -vf scale='min(max_edge,iw)':-1 -f image2 <tmp>.png`
  (seek ~10% in to avoid the black lead-in frame; fall back to `-ss 0` for very short clips), then read the temp
  PNG back into a `DynamicImage`. Clean up the temp file. Never panic; missing/failed ffmpeg or an undecodable
  file → `Err`. Resolve the ffmpeg binary path (bundled resource path first, then PATH as a dev fallback).
- `crates/server/Cargo.toml`: add `[features] video-thumb = []` (NO crate dep — uses `std::process::Command`), off by default.
- `crates/server/src/lib.rs`: `#[cfg(feature="video-thumb")] pub mod thumb_video;`.
- `crates/server/src/thumb_source.rs`: add the video dispatch **EARLY — before `fs::read`** (never slurp a
  multi-GB video into memory): `#[cfg(feature="video-thumb")]` match on video extensions (mp4/mov/mkv/webm/avi/m4v/…)
  → `thumb_video::extract_frame(path, max_edge)`.
- `src-tauri/tauri.sidecar.windows.conf.json` + `tauri.sidecar.unix.conf.json`: add the ffmpeg binary as a
  bundle resource (path only — actual binary acquisition wired in CPE-1258).
- Cargo tests: **ffmpeg IS present locally (v8.1.1)** so a real-render test can generate a tiny synthetic clip
  (or use a committed tiny fixture) and assert a non-degenerate frame; gate the real-render test behind an
  ffmpeg-presence check so it skips (not fails) on a runner without ffmpeg; keep the missing-ffmpeg→`Err` and
  undecodable→`Err` tests unconditional.

## Acceptance criteria
- `cargo build`/`test`/`clippy --all-targets -D warnings` clean both with and without `--features video-thumb`.
- `decode_thumb_image` returns a real frame for a video when the feature is on + ffmpeg present; Err→icon otherwise.
- Video path never `fs::read`s the whole file. No new crate dep. No panic on any input.

## Notes
Sequence AFTER CPE-1256 (shares additive lines in thumb_source.rs/Cargo.toml/lib.rs). Ship-enablement +
release binary bundling is CPE-1258.

## Work Log
- 2026-08-02: Implemented `crates/server/src/thumb_video.rs` (`extract_frame`, resolves a bundled
  ffmpeg next to the exe then falls back to PATH; shells ffmpeg via `std::process::Command`, seeks
  ~1s in with a `-ss 0` retry, writes to a unique temp PNG, reads it back via `image::open`, deletes
  the temp file on both the success and error path, then applies a final exact longest-edge downscale
  that never upscales). Added `[features] video-thumb = []` to `crates/server/Cargo.toml` (zero new
  Cargo deps — confirmed via `cargo tree` diff, identical with the feature on/off and with/without
  `pdf-thumb`). Wired `#[cfg(feature = "video-thumb")] pub mod thumb_video;` in `lib.rs`, and an early
  dispatch in `thumb_source::decode_thumb_image` (before `fs::read`) matching
  `thumb_video::VIDEO_EXTENSIONS` (mp4/mov/mkv/webm/avi/m4v/mpg/mpeg/wmv/flv). Added placeholder ffmpeg
  bundle-resource entries to both sidecar conf JSONs (path only; real binary acquisition is CPE-1258).
  Verified: `cargo build` / `cargo build --features video-thumb` / `cargo build --features
  pdf-thumb,video-thumb` all clean; `cargo test --lib` (1262 passed) and `cargo test --lib --features
  video-thumb` (1269 passed, incl. all 7 `thumb_video` tests — ffmpeg 8.1.1 is installed locally so
  both real-render tests ran for real, not skipped) both green; `cargo clippy --all-targets -D
  warnings` clean with and without `--features video-thumb`.

## Work Log
- 2026-08-02 — Worker (sonnet, worktree) built thumb_video.rs: shells bundled/PATH ffmpeg via vector args
  (injection-safe), seeks ~1s (fallback 0), unique temp PNG (pid+nanos+atomic counter), cleanup on all paths,
  exact non-upscaling longest-edge downscale. video-thumb feature = ZERO deps. Early dispatch before fs::read.
- Verify: build clean all modes; 1262 off / 1269 on tests, ALL 7 video tests ran REAL (ffmpeg 8.1.1 local);
  clippy clean both modes; cargo tree byte-identical on/off (dep-free).
- Independent OPUS Reviewer re-ran 9 checks + point-by-point (injection SAFE, no-slurp, panic-safe, temp cleanup,
  downscale, feature-gate, graceful degrade) → APPROVE. Merged PR #559 (squash cbb95a7d, --admin: CI stalled).
- Non-blocking review finding → filed CPE-1261: temp name is unique but predictable (CWE-377 symlink-clobber
  window on shared /tmp, Linux only; Win/macOS per-user temp unaffected). Fast-follow hardening.
