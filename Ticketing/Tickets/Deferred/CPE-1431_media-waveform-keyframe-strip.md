---
id: CPE-1431
title: "Waveform / keyframe scrub strip for the media player"
type: Feature
status: Deferred
priority: Low
component: Multiple
tags: [deferred-internal]
epic: CPE-720
created: 2026-08-07
---
## Scope
Enrich the media transport (CPE-1429/1430) with a visual scrub aid:
- **Audio:** a waveform rendered under/over the scrub bar.
- **Video:** a keyframe/thumbnail strip for hover-frame scrubbing.

Extraction is non-trivial (decode/sample the media, cache the result — reuse the thumbnail pipeline **CPE-718**).
Weigh against PURPOSE.md's fast/small/predictable tiebreaker: cache aggressively, do it off the main thread, and
add no cost when the media pane is closed.

## Why deferred
The epic's core DoD (playback + transport + full-screen quick-look) ships in CPE-1429/1430 without this. The
waveform/keyframe strip is a heavier nice-to-have (extraction + caching); pick it up once the core lands and only
if it earns its cost. Revisit after CPE-1429/1430 merge.
