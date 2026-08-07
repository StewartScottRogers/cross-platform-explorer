---
title: Media Player
order: 29
category: Explorer
categoryOrder: 2
---

# Media Player

Select an audio or video file and the preview pane plays it right there, with a compact custom
transport — no need to open another app just to hear or watch a clip.

## What plays

The player uses the formats the app's built-in web engine can decode natively, so nothing extra has to
be installed:

- **Audio** — MP3, WAV, OGG, FLAC, M4A, AAC, Opus
- **Video** — MP4, WebM, MOV

An `.ogg` file is treated as audio by default. Other containers a browser engine doesn't decode (for
example Matroska `.mkv`, `.avi`, or `.wmv`) may not play — see *When a file won't play* below.

## The transport

The pane shows its own themed controls instead of the raw browser bar:

- **Play / Pause** — start and stop playback.
- **Scrub bar** — drag to seek; the current time and total duration sit on either side.
- **Volume + Mute** — set the level, or mute without losing your place; raising the slider un-mutes.
- **Speed** — click to step through 0.5×, 0.75×, 1×, 1.25×, 1.5× and 2×, wrapping back to 0.5×.
- **Loop** — toggle whether the clip restarts automatically when it reaches the end.

The controls wrap onto a second row in a narrow pane rather than clipping.

## When a file won't play

If the codec or container isn't supported, the pane shows a short message and an **Open externally**
button that hands the file to your operating system's default media application. Nothing crashes, and
selecting anything else clears the message.

## Notes

- Video streams as it plays — a large file starts without waiting for the whole thing to load, and only
  costs anything while a media file is actually selected.
- Playback state (position, volume, speed, loop) is per-file and resets when you pick a different clip.
