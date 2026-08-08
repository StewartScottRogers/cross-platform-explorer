---
id: CPE-1480
title: "Harden thumb_video ffmpeg invocation with -protocol_whitelist (same SSRF/protocol-injection gap fixed in CPE-1478)"
type: Bug
status: Doing
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
