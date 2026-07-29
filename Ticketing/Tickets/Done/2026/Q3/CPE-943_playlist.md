---
id: CPE-943
title: Media playlist / queue navigation model
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-720
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
First headless slice of the audio/video player pane (CPE-720). `cpe_server::playlist`:
- `Playlist::new(items)` — an ordered track list + a cursor.
- `current()` / `next()` / `prev()` — navigation honouring `RepeatMode::{Off, One, All}` (Off stops at the
  ends, One repeats the current track, All wraps).
- `set_shuffle(on, seed)` — a **deterministic** (seeded Fisher-Yates) play order that keeps the current
  track under the cursor across the toggle; `select(index)` jumps the cursor.

Pure navigation only — no decoding/playback; the player pane drives it and renders `current()`.

## Acceptance Criteria
- [x] next/prev honour Off/One/All correctly; empty playlist is safe.
- [x] Shuffle is a full permutation, preserves the current track, and is reversible. 5 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Activated CPE-720 with the playlist navigation core. The actual audio/video
  decode + transport UI, and format support, are the remaining children.
