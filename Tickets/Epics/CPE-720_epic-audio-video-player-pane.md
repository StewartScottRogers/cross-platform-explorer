---
id: CPE-720
title: "EPIC: Audio & video player pane"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Turn the preview pane into a real media transport for audio and video: play/pause, a seekable scrub bar
with hover-frame/waveform, volume, playback speed, loop, and thumbnail-strip scrubbing for video — plus a
spacebar quick-look full-screen player that steps through the folder's media with arrow keys.

## Why
The app has zero temporal-media playback today. Previewing audio/video in place (and stepping through a
folder of clips) is a basic expectation for a general explorer.

## Rough scope (areas, not child tickets)
- Media format probing and playback in the webview, with codec/container awareness.
- A transport UI (scrub/volume/speed/loop) and a waveform / keyframe strip.
- A full-screen quick-look player with next/prev stepping across the folder's media.
- Fallbacks for unsupported codecs (message + external-open).

## Open questions (resolve at activation)
- Codec coverage in the webview vs. a bundled decoder; licensing/size implications.
- Waveform/keyframe extraction cost and caching (reuse the thumbnail pipeline [[CPE-718]]?).
- Overlap with the spacebar quick-look overlay used by other preview types.

## Definition of Done
- Audio and video play in the preview pane with a working transport and scrub.
- A full-screen quick-look player steps through the folder's media with the keyboard.
- Unsupported formats degrade gracefully; no cost when no media is selected.
