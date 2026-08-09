---
id: CPE-1478
title: "Backend audio-waveform-peaks extraction command (epic CPE-720) — decode via bundled ffmpeg subprocess, no new dep"
type: Feature
status: Done
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
Scoped by the sprint Planner 2026-08-08; disjoint from the concurrent sprints_* work.

## Work Log

### 2026-08-08 — Implemented + shipped

**Extraction (`crates/server/src/ffmpeg_util.rs`, new):** factored `resolve_ffmpeg_bin`,
`set_native_dep_dir`/`NATIVE_DEP_DIR`, `create_scratch_dir` (now takes a caller `tag`, e.g.
`"thumbvideo"`/`"waveform"`, so each caller's temp-dir names stay distinguishable), and the test-only
`ffmpeg_available`/`last_scratch_dir_for_test` out of `thumb_video.rs` into this new shared module, gated
`#[cfg(any(feature = "video-thumb", feature = "waveform"))]`. `thumb_video.rs` now calls into it instead
of owning private copies; its own test suite (14 tests) still passes unchanged, minus one now-redundant
concurrency test (`create_scratch_dir_never_collides_across_concurrent_calls`) that duplicated a test now
living directly against the shared implementation in `ffmpeg_util`'s own test module. The app adapter
(`src-tauri/src/lib.rs`'s `init_thumbnail_native_dep_dir`) now calls `cpe_server::ffmpeg_util::
set_native_dep_dir` once instead of `thumb_video::set_native_dep_dir` — a single injection now covers the
bundled ffmpeg's resource dir for BOTH `thumb_video` and `media_waveform`.

**New module (`crates/server/src/media_waveform.rs`):** `pub fn extract_waveform_peaks(path: &Path,
buckets: usize) -> Result<Vec<(f32, f32)>, String>`. Decodes `ffmpeg -nostdin -hide_banner -loglevel
error -i <path> -vn -f f32le -ac 1 -ar 8000 pipe:1` and buckets the PCM into exactly `buckets` `(min,
max)` pairs in ascending time order (`buckets` clamped to at least 1).

**Decide-and-log — sample rate + byte cap:** chose **8 kHz** mono decode (a waveform envelope only needs
the min/max shape, not audio fidelity — lower rate means more real duration fits under the byte cap) and a
**64 MiB** PCM cap (`MAX_PCM_BYTES`), mirroring the `MAX_SOURCE_FILE_BYTES` (128 MiB, `thumb_source`) /
`MAX_SFNT_BYTES` (64 MiB, `thumb_font`) caps already established in this crate. At 8 kHz mono `f32le` (4
bytes/sample) that's ~35 minutes of decoded audio — generous headroom for any real music/voice-memo/
podcast file while bounding worst-case memory to one fixed budget regardless of a crafted file's declared
or actual duration.

**CRITICAL bounded-read design:** never `Command::output()`/`wait_with_output()` (both buffer the whole
child stdout — unbounded). Instead `read_capped<R: Read>(r: R, cap: u64) -> Result<(Vec<u8>, bool),
String>` wraps the pipe in `Read::take(cap + 1)` and truncates back down, mirroring
`doc_text::read_entry_capped`'s cap-plus-one idiom (disambiguates an exact-cap-sized stream from a
truncated one). Made `Read`-generic specifically so the bounding logic is unit-tested directly against an
in-memory `Cursor` (`read_capped_never_buffers_more_than_the_cap_plus_one_byte_on_a_huge_input` proves a 10
MiB input against a 1 KiB test cap is never buffered past `cap + 1` bytes) without needing a real ffmpeg
subprocess to actually emit gigabytes. stderr is drained on its own thread (also `read_capped`, 64 KiB
cap) CONCURRENTLY with the bounded stdout read — draining only one pipe risks a classic subprocess
deadlock if the other fills. If the stdout cap was hit, the child is killed explicitly afterward (it would
otherwise block forever writing into a pipe nobody is reading); exit-status is only enforced when the
whole stream was consumed (a truncated read means WE killed it, so a non-zero/killed status there is
expected, not a real failure — the truncated PCM already captured is still valid).

**Feature wiring:** `waveform = []` in `crates/server/Cargo.toml` (no `dep:`, zero new Cargo deps, mirrors
`video-thumb`); `#[cfg(feature = "waveform")] pub mod media_waveform;` in `lib.rs`; `waveform` added to the
`cpe-server` feature list in `src-tauri/Cargo.toml` (alongside `video-thumb`, reusing the same bundled
ffmpeg — no new native dep to ship); one new command `audio_waveform_peaks(path: String, buckets: usize)
-> Result<Vec<(f32, f32)>, String>` in `src-tauri/src/lib.rs`, `spawn_blocking`-wrapped, registered in both
`generate_handler![]` and the `export_bindings` `collect_commands![]` list. Also added `waveform` to the
CI server-clippy/-test combined-features line (`.github/workflows/ci.yml`) alongside `pdf-thumb,
video-thumb,dicom-thumb` so CI's installed ffmpeg exercises the real-render tests for real (not skip).

**Tests:** mirrored `thumb_video.rs`'s structure — unconditional: missing/bogus ffmpeg binary, garbage
bytes, nonexistent path, 0-byte file, zero-bucket-request all → `Err` not panic; plus the `read_capped`
bounding tests (huge-input truncation, under-cap passthrough, exact-cap-not-misreported) and 6
`bucket_pcm_f32le` unit tests (exact bucket count, true min/max within a span, more-buckets-than-samples,
empty PCM, trailing partial sample, NaN/Infinity-reinterpreted-bytes guard). ffmpeg-gated (SKIP not fail
via `ffmpeg_util::ffmpeg_available()`): synthesizes a `sine=frequency=440:duration=N` lavfi fixture at test
time (not a committed binary), asserts bucket count == requested + a non-degenerate amplitude envelope
(max abs > 0.1, sanity-capped ≤ 2.0), and that a 4x-longer synthesized source downsamples to the SAME
bucket count.

**Verification results (ffmpeg IS installed locally, so the gated tests ran for real — no skips):**
- `cargo build` (crates/server, default + `--features waveform` + `--features video-thumb,waveform`): clean.
- `cargo test` crates/server default: 1736 passed, 0 failed (unchanged from pre-change baseline minus the
  one removed redundant test). `--features waveform`: 1756 passed (+20 = 4 new `ffmpeg_util` tests + 16
  new `media_waveform` tests), 0 failed. `--features video-thumb,waveform`: 1766 passed (+10 `thumb_video`
  tests), 0 failed. No skipped/panicked lines in any run.
- `cargo clippy --all-targets -D warnings` crates/server: clean for default, `--features waveform`, and
  `--features video-thumb,waveform`.
- `cargo build` src-tauri (default features, which include `waveform`): clean.
- `cargo clippy --all-targets -D warnings` src-tauri: clean for default and `--features sidecar-platform`.
- `bindings.gen.ts` regenerated (`cargo run --bin export_bindings --features "specta-bindings
  sidecar-platform"`) — additive-only 19-line diff adding `audioWaveformPeaks`; nothing else drifted.
- `npm run check` NOT run (no `node_modules` in the fresh worktree, and no frontend code was touched —
  this ticket is explicitly backend-only, GUI consumer is a separate later ticket per scope).

**For Reviewer/UAT to scrutinize:** the bounded-read correctness (kill-after-truncate + concurrent stderr
drain avoiding the classic subprocess deadlock) and the NaN/Infinity guard in `bucket_pcm_f32le` are the
two places most worth a second look given this is new engineering risk `thumb_video` never had (per the
ticket's own framing). `SAMPLE_RATE`/`MAX_PCM_BYTES` are a judgment call (see decide-and-log above) with
no committed real-audio fixture to validate against ear/eye — only synthetic lavfi sine waves.
