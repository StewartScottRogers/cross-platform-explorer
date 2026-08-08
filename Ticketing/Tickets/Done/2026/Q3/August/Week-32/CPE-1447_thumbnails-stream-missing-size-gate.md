---
id: CPE-1447
title: "thumbnails_stream skips the 128 MiB size gate the single-thumbnail command applies → OOM on a huge image file"
type: Bug
status: Done
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

## Work Log (2026-08-07)

### Attempt 1 (superseded)
Landed the gate in `src-tauri/src/lib.rs` (a `thumb_compute` helper calling the app layer's existing
`ensure_previewable_size(path, PREVIEW_INFO_MAX_BYTES)` before dispatch) and mirrored the same call in
the single `thumbnail` command. Independent review on PR #711 caught a real regression: that gate ran
BEFORE `decode_thumb_image`'s extension dispatch, so it fired for VIDEO extensions too — but the video
branch (`thumb_video::extract_frame`, shelling out to ffmpeg) never reads the file into memory in the
first place, and videos routinely exceed 128 MiB. The unconditional gate therefore blocked legitimate
video thumbnails in the grid, a regression from pre-PR behavior (the streaming path had no gate at all
before this ticket, so videos thumbnailed fine). None of attempt 1's three tests used a video extension,
so it slipped through. This is exactly the gap ticket **CPE-1449** ("large video files get no frame
thumbnail") had independently identified against the single `thumbnail` command's pre-existing instance
of the same bug.

### Attempt 2 (shipped) — gate moved into `cpe-server`, subsumes CPE-1449
**Where the gate now lives.** `crates/server/src/thumb_source.rs`, inside `decode_thumb_image` itself,
placed immediately AFTER the `#[cfg(feature = "video-thumb")]` video early-return and BEFORE the
`std::fs::read(path)` that follows it. A new module-private constant, `MAX_SOURCE_FILE_BYTES: u64 = 128
* 1024 * 1024`, matches the value the app-layer `PREVIEW_INFO_MAX_BYTES` uses (same 128 MiB budget, kept
as a documented local copy since `cpe-server` is Tauri-free and can't reference the app crate's
private const). The check is a plain `fs::metadata(path).len() > MAX_SOURCE_FILE_BYTES` — no bytes
read — mirroring `ensure_previewable_size`'s exact semantics (a stat error is left for the `fs::read`
below to report, not treated as a gate failure).

**How video bypasses it.** `decode_thumb_image` dispatches `VIDEO_EXTENSIONS` (mp4/mov/mkv/webm/avi/
m4v/mpg/mpeg/wmv/flv) to `thumb_video::extract_frame` and `return`s EARLY, before the new gate is even
reached — the gate sits textually and logically after that return, so a video file's code path never
executes it regardless of file size. Non-video (raster/PSD/SVG/font/PDF) extensions all still fall
through to the unconditional `fs::read` a few lines down, so they're gated exactly as before.

**Both entry points fixed.** `decode_thumb_image` is the ONE shared call site both `thumbnail_cached`/
`make_thumbnail_png` funnel through, and both the single `thumbnail` command and the streaming
`thumbnails_stream` batch call those — so gating inside `decode_thumb_image` fixes both automatically
and by construction, with no way for the two paths to drift again. Removed the now-redundant
`ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?` call from the single `thumbnail` command
(`src-tauri/src/lib.rs:~1210`) and deleted the `thumb_compute` wrapper + its call site in
`thumbnails_stream`, reverting that closure to call `thumbnail_cached`/`make_thumbnail_png` directly —
there is no lib.rs-side gate left at all; `ensure_previewable_size`/`PREVIEW_INFO_MAX_BYTES` themselves
are untouched and still used by the many *other* preview readers (PE/wasm/torrent/cert/etc.) that have
nothing to do with thumbnails.

**CPE-1449 resolved by this fix.** Because the gate now sits after the video dispatch for BOTH entry
points, the single `thumbnail` command's pre-existing video-over-block (CPE-1449's actual complaint,
filed against `~:1212` before this PR even existed) is fixed for free, not just the streaming
regression attempt 1 introduced. `Ticketing/Tickets/Backlog/CPE-1449_video-thumb-overblocked-by-size-gate.md`
moved to Done alongside this ticket, noting it as subsumed by this PR.

**Tests:**
- `crates/server/src/thumb_source.rs::decode_thumb_image_rejects_an_oversize_raster_source_before_reading_it`
  — a sparse (`File::set_len`, no real bytes written) `.png` one byte over `MAX_SOURCE_FILE_BYTES` is
  refused with a "too large" error — the actual CPE-1447 OOM fix, now proven at the real 128 MiB cap
  (attempt 1's version used a tiny cap parameter that no longer exists).
- `crates/server/src/thumb_source.rs::decode_thumb_image_does_not_size_gate_a_video_extension`
  (`#[cfg(feature = "video-thumb")]`) — a sparse `.mp4` far over the cap must NOT be rejected with the
  size-gate's "too large" message (whatever OTHER error comes back — missing ffmpeg, undecodable
  content — is fine; only the size-gate message specifically would fail this test), proving the video
  branch never reaches the gate. This is the CPE-1449 regression pin.
- `src-tauri/src/lib.rs::thumbnails_stream_pipeline_skips_oversize_and_thumbnails_normal_in_the_same_batch`
  (kept, updated) — drives the real `cpe_server::thumb_pipeline::run_thumb_batch` with the production
  closure shape (`thumbnail_cached`/`make_thumbnail_png` directly, no lib.rs wrapper): a sparse
  >128 MiB `.png` in the same batch as a normal small `.png` comes back `data_url: None` while the
  normal file still thumbnails.
- Removed attempt 1's `thumb_compute_rejects_an_oversize_file_before_reading_it` and
  `thumb_compute_still_thumbnails_a_normal_size_file` (the function they tested no longer exists;
  superseded by the two `thumb_source` tests above, which test the real gate location).

**Verification run (attempt 2):**
- `cargo build` (src-tauri) — clean.
- `cargo clippy --all-targets -- -D warnings` (src-tauri, default features) — clean (one redundant-
  closure lint fixed along the way: `cpe_server::thumbnail::make_thumbnail_png` passed directly instead
  of wrapped in a closure).
- `cargo clippy --all-targets --features specta-bindings -- -D warnings` (src-tauri) — clean.
- `cargo test` (src-tauri) — 135 passed, 0 failed (137 minus the 2 removed `thumb_compute` tests).
- `cargo clippy --all-targets -- -D warnings` (crates/server, default features) — clean.
- `cargo clippy --all-targets --features index -- -D warnings` (crates/server) — clean.
- `cargo clippy --all-targets --features pdf-thumb,video-thumb,dicom-thumb -- -D warnings`
  (crates/server) — clean.
- `cargo test` (crates/server, default features) — 1702 passed, 0 failed, 1 ignored (pre-existing,
  unrelated).
- `cargo test --features pdf-thumb,video-thumb,dicom-thumb thumb_source` (crates/server) — 9 passed, 0
  failed, including the new video-bypass regression pin.
- No `specta::Type` struct touched anywhere in either attempt — no bindings regen needed.

**PR:** #711 (branch `cpe-1447-thumbnails-stream-size-gate`), attempt 2 pushed as follow-up commits on
the same branch/PR per the reviewer's request.
