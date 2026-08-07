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

## Full-screen quick-look

Press **Space** with an audio or video file selected to open it in a full-screen player, front and
centre over the window — handy for actually watching a clip rather than glancing at it in the side
pane. Press **Space** again, or **Esc**, or click the dimmed area outside the player, to close and
return to the file list.

### Stepping through a folder

Once the player is open, the **←** and **→** arrow keys (or the on-screen previous/next buttons) move
to the previous and next media file **in the same folder** — the folder's audio and video files, in the
order they're listed. The stepped-to clip starts playing automatically.

Two controls at the bottom shape that order:

- **Repeat** — click to cycle **off → all → one**. *Off* stops at the first and last file; *all* wraps
  around from the end back to the start (and vice-versa); *one* stays on the current file.
- **Shuffle** — toggle a randomised walk through the folder's media; toggling it back off restores the
  listed order, keeping whatever file is playing.

The same themed transport (play/pause, scrub, volume, speed, loop) described above sits inside the
full-screen player, so nothing about the controls changes — only the size.

## When a file won't play

If the codec or container isn't supported, the pane shows a short message and an **Open externally**
button that hands the file to your operating system's default media application. Nothing crashes, and
selecting anything else clears the message.

## Notes

- Video streams as it plays — a large file starts without waiting for the whole thing to load, and only
  costs anything while a media file is actually selected.
- Playback state (position, volume, speed, loop) is per-file and resets when you pick a different clip.
