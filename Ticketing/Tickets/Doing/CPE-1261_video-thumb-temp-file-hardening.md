---
id: CPE-1261
title: "Harden video-thumbnail temp file against symlink pre-plant (CWE-377) on shared /tmp"
type: chore
component: cpe-server
priority: low
status: Doing
tags: ready
created: 2026-08-02
epic: CPE-718
---

## Summary
Non-blocking finding from the CPE-1257 review (PR #559). `thumb_video::extract_frame` writes ffmpeg's output
to a temp PNG whose name is **unique but predictable** (`temp_dir()/cpe-thumbvideo-{pid}-{nanos}-{counter}.png`).
On a shared, world-writable `/tmp` (Linux) an attacker who can guess the name could pre-plant a symlink there;
ffmpeg runs with `-y` (O_TRUNC, follows symlinks) → a clobber window (CWE-377). Windows/macOS use per-user temp
dirs, so the primary ship targets are unaffected; the concurrency-collision case the original ticket specified
is already solved by the atomic counter.

## Build (dep-free)
- Replace the predictable single-file temp with an **exclusively-created per-invocation scratch dir**: attempt
  `std::fs::create_dir(temp_dir()/cpe-thumbvideo-{pid}-{nanos}-{counter})` — `create_dir` fails atomically if the
  path already exists (including a pre-planted dir/symlink), so success means we own a fresh directory. Write the
  PNG inside it. On cleanup `remove_dir_all` the whole scratch dir (on success and every error path).
- Alternatively add high-entropy randomness to the filename dep-free (e.g. hash of an OS-provided source), but the
  exclusive-dir approach is simpler and closes the window deterministically.
- Keep it feature-gated under `video-thumb`; no new crate dep.

## Acceptance criteria
- Temp path is no longer clobberable via a pre-planted symlink (exclusive create or high-entropy name).
- Cleanup still runs on success + all error paths (no leak); concurrent calls still never collide.
- `cargo test`/`clippy --all-targets -D warnings` clean with `--features video-thumb`; existing thumb_video tests still pass.

## Notes
Low priority / Linux-only. Fold into shift A's tail or a later hardening pass.
