---
id: CPE-1480
title: "Harden thumb_video ffmpeg invocation with -protocol_whitelist (same SSRF/protocol-injection gap fixed in CPE-1478)"
type: Bug
status: Done
priority: Medium
component: Backend
tags: [ready, security]
epic: CPE-718
created: 2026-08-08
---
## Vector (found by the CPE-1478 adversarial review, 2026-08-08)
`crates/server/src/thumb_video.rs` builds `ffmpeg -i <path> …` (~lines 130-144) from a path string without a
`-protocol_whitelist` and without a pre-spawn regular-file check — the **identical** latent SSRF/protocol-injection
gap that was empirically confirmed and fixed for the new waveform command in **CPE-1478**. `ffmpeg -i` treats its
input as a *protocol*, so a `path` of `http://169.254.169.254/…` (blind SSRF to internal hosts / cloud-metadata),
`concat:…`, `subfile:…`, or `data:…` is honored rather than being rejected as a filename.

## Not exploitable today (defense-in-depth)
`thumb_video` at least dispatches on `VIDEO_EXTENSIONS`, and the only caller is the frontend passing user-selected
local paths — reaching this needs a second bug (an XSS / compromised webview). But it is an IPC-reachable subprocess
boundary on a file-explorer that previews untrusted content, and the fix is one line. Fixing it keeps the whole
ffmpeg-subprocess boundary consistent after CPE-1478 hardened its twin next door.

## Fix (mirror CPE-1478's `media_waveform.rs`)
In `extract_video_thumbnail`'s ffmpeg arg build, before `-i`:
- add `.arg("-protocol_whitelist").arg("file,pipe")` (load-bearing guard at the ffmpeg layer), and
- optionally reject a non-regular-file path pre-spawn with `std::fs::metadata(path).map(|m| m.is_file())`.
Consider factoring a tiny shared `ffmpeg_util::base_input_args()` (or similar) so `media_waveform` and `thumb_video`
share ONE hardened input-arg builder instead of two copies — CPE-1478 already extracted `ffmpeg_util.rs`, so this is
the natural home. Add a regression test that a `http://…`/`concat:…` input is rejected (mirror CPE-1478's
`extract_waveform_peaks_rejects_a_non_file_protocol_string_without_reaching_the_network`).

## Verification
Headless: `cargo test --features video-thumb` (existing thumb_video tests stay green + the new rejection test),
`cargo clippy --all-targets -D warnings` both feature modes. ffmpeg is available locally for the real-render tests.

## Notes
Filed from the CPE-1478 workshift review. Epic CPE-718 (universal thumbnail pipeline). Low blast radius, disjoint
from the concurrent workshifts_* work.

## Work Log

### 2026-08-08 — Worker
Implemented as specified, with the shared-helper factoring the ticket preferred over inline copy-paste:

- **`crates/server/src/ffmpeg_util.rs`**: added `pub const FFMPEG_PROTOCOL_WHITELIST: &str = "file,pipe"` and
  `pub fn reject_unsafe_ffmpeg_input(path: &Path) -> Result<(), String>` (the `std::fs::metadata(path).map(|m|
  m.is_file())` pre-spawn guard CPE-1478 wrote inline in `media_waveform.rs`, now factored out so both
  ffmpeg-shelling modules share ONE hardened input-check instead of two independently-maintained copies). Added
  4 new unit tests for the helper (rejects non-file-protocol strings incl. `http://`/`concat:`/`subfile:`,
  rejects a nonexistent path, rejects a directory, accepts a real file).
- **`crates/server/src/media_waveform.rs`**: refactored its existing CPE-1478 inline guard + literal
  `"file,pipe"` to call the new shared `reject_unsafe_ffmpeg_input` / `FFMPEG_PROTOCOL_WHITELIST` instead —
  behavior unchanged, same tests still pass.
- **`crates/server/src/thumb_video.rs`** (the actual gap this ticket closes): in `extract_frame_with_ffmpeg`,
  added the `reject_unsafe_ffmpeg_input(path)?` pre-spawn guard before creating the scratch dir (fails fast,
  skips scratch-dir creation for a doomed request). In `run_ffmpeg_frame`'s `Command` build, added
  `.arg("-protocol_whitelist").arg(FFMPEG_PROTOCOL_WHITELIST)` before `-i`, with a comment mirroring
  `media_waveform`'s SSRF/protocol-injection rationale. Added the regression test
  `extract_frame_rejects_a_non_file_protocol_string_without_reaching_the_network` (mirrors
  `media_waveform`'s CPE-1478 test), asserting `http://169.254.169.254/…`, `concat:/etc/passwd`, and
  `subfile:,start,0,end,64,,:/etc/passwd` are all rejected without any ffmpeg install needed (the guard fires
  before spawn). Existing `thumb_video` behavior for legit local video files is unchanged.

**Verification (Z: drive worktree, not the Temp scratchpad — the Temp path hit an unrelated `LINK : fatal
error LNK1104` file-lock on two `tauri-plugin-*` build scripts, most likely AV scanning; building the exact
same code from `Z:\repos\cpe-1480-worktree` succeeded cleanly, confirming it was an environment/path issue, not
a code issue):**
- `cargo build` (src-tauri, default profile) — OK, `cross-platform-explorer v0.57.61` + `cpe-server` compiled.
- `cargo test --features video-thumb` (crates/server) — **1755 passed** in the main lib target (0 failed), plus
  21/17/2/1/1/45/14/32 passed across the other integration-test binaries — 0 failures anywhere. ffmpeg was
  available locally so every real-render/ffmpeg-gated test (incl. the two new/refactored ones) actually ran
  rather than skipping.
- `cargo clippy --all-targets -- -D warnings` (default features) — clean.
- `cargo clippy --all-targets --features video-thumb -- -D warnings` — clean.
- `cargo clippy --all-targets --features waveform -- -D warnings` — clean.
- `cargo clippy --all-targets --features video-thumb,waveform -- -D warnings` — clean.

No `specta::Type`/command-signature change; `bindings.gen.ts` regen not needed.
