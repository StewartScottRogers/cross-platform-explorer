---
id: CPE-1478
title: "Backend audio-waveform-peaks extraction command (epic CPE-720) — decode via bundled ffmpeg subprocess, no new dep"
type: Feature
status: Doing
priority: Medium
component: Backend
tags: [ready]
epic: CPE-720
parent: CPE-1431
created: 2026-08-08
---
## What
The first concrete backend deliverable of the audio/video player pane's waveform strip (CPE-1431 / epic
CPE-720): a pure `cpe-server` command that turns an audio file into a **downsampled peak array** — a fixed
bucket count of `(min, max)` (or `(peak, rms)`) samples regardless of file length — for a later GUI ticket to
render as a scrub-bar waveform. Backend-first landing is the established pattern for this epic (CPE-943
Playlist landed before CPE-1429/1430 consumed it); this command has no user-visible payoff until its GUI
consumer is scheduled, which is fine and expected.

## How — reuse the blessed external-ffmpeg-subprocess exception (NO new Cargo crate)
`cpe-server` has **no in-process audio/video decode crate** and must not add one (lean-core + the ffmpeg-binding
LGPL/GPL license concern — see research-library entry `thumbnail-native-deps-pdf-video-2026-08-02.md`). Instead
reuse the already-shipped, license-clean pattern in `crates/server/src/thumb_video.rs` (CPE-1257/1258): shell
out to the **separately-bundled `ffmpeg` executable** as a subprocess (`std::process::Command`, never linked
in-process), behind an off-by-default `= []` Cargo feature that adds **zero** new dependencies (mirror
`video-thumb`).

- **Decode:** pipe mono PCM out of ffmpeg — `ffmpeg -i <path> -f f32le -ac 1 -ar <rate> pipe:1` — and compute
  the min/max (and/or RMS) per bucket in Rust. This covers the full CPE-1429 format list
  (mp3/wav/ogg/flac/m4a/aac/opus), unlike a WAV-only pure-Rust parser.
- **Factor the shared plumbing** (`resolve_ffmpeg_bin`, `set_native_dep_dir`, `create_scratch_dir`) out of
  `thumb_video.rs` into a small shared `ffmpeg_util.rs` rather than a third copy — those helpers are currently
  private to `thumb_video.rs`.

## Module + signature
- `crates/server/src/media_waveform.rs` —
  `pub fn extract_waveform_peaks(path: &Path, buckets: usize) -> Result<Vec<(f32, f32)>, String>`
  (downsampled to exactly `buckets` regardless of source length; ascending time order).
- `crates/server/src/lib.rs`: `pub mod media_waveform;` + `pub mod ffmpeg_util;`.
- `crates/server/Cargo.toml`: new `waveform = []` feature (no `dep:`), mirroring `video-thumb`.
- `src-tauri/Cargo.toml` (~line 58): add the feature to the `cpe-server` dep feature list next to `video-thumb`.
- `src-tauri/src/lib.rs`: one new `#[tauri::command]`, `spawn_blocking`-wrapped, one-line dispatch into the
  module fn (thin-dispatcher convention). Register it in `generate_handler![]`.

## CRITICAL: bounded read (do NOT use Command::output())
The PCM pipe is genuinely new engineering risk `thumb_video.rs` never had (it only read back a small PNG). A
long/crafted audio file yields a huge PCM stream — reading it with `Command::output()`'s unbounded buffer is an
OOM DoS. **Stream ffmpeg's stdout with a bounded reader** (cap total bytes read, mirror the
`MAX_SOURCE_FILE_BYTES`/bounded-read convention already in the codebase) and bucket incrementally, or cap
decode duration with `-t`. This must be designed + tested, not hand-waved. Follows the streaming +
resource-exhaustion conventions (see STREAMING.md and the prior DoS-hardening sweeps).

## Headless verification plan (mirror thumb_video.rs's test structure)
- **Unconditional (no ffmpeg needed):** missing/bogus ffmpeg binary → `Err` not panic; garbage bytes → `Err`;
  nonexistent path → `Err`; empty (0-byte) file → `Err`; huge/undecodable input stays bounded (no OOM/panic).
- **ffmpeg-gated (skip, don't fail, when unavailable — reuse the `ffmpeg_available()` pattern):** synthesize a
  fixture at test time with ffmpeg's own `lavfi` source (`-f lavfi -i sine=frequency=440:duration=1`), NOT a
  committed binary fixture. Assert: bucket count == requested; values non-degenerate + within amplitude range
  (not all-zero); a longer synthesized file downsamples to the same bucket count.
- `cargo test` + `clippy --all-targets -D warnings` BOTH feature modes (default and `--features waveform`).

## Out of scope (follow-up siblings under CPE-1431)
- Video **keyframe-timestamp** extraction (different ffmpeg invocation + fragile `showinfo` stderr parsing) —
  file separately.
- The GUI scrub-bar / waveform-strip rendering that consumes this array — user-facing, a later slice.

## Effort / tier
Medium — plumbing is solved + reusable, but the bounded PCM streaming + bucketing algorithm + feature/dispatcher
wiring + fixture-synthesis tests are real work (same complexity class as CPE-1257). Standard worker tier.
Scoped by the workshift Planner 2026-08-08; disjoint from the concurrent workshifts_* work.
