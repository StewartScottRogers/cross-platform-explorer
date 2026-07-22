---
id: CPE-563
title: "Fix: CPE-559 audio/video categories broke the read-only 'info' preview (mid/midi)"
type: Bug
status: Done
priority: High
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
CPE-559 added `mid`/`midi`/`wma`/`aiff`/`mpg`/`mpeg`/`3gp`/`mts`/`m2ts` to `CATEGORY_BY_EXT` as
`audio`/`video`. But `CATEGORY_BY_EXT` drives **both** the file icon **and** the preview provider, and the
audio/video providers (which match by category and precede the `info` provider) then claimed those files —
so `mid`/`midi` (explicit `INFO_EXT` formats) previewed as a broken media player instead of read-only info
text, and non-web audio/video formats would show an empty player. CI caught it: `provider.test.ts` →
`expected 'audio' to be 'info'`. It slipped locally because the full suite wasn't re-run after CPE-559
(only `filetypes.test.ts` was).

## Resolution
Removed the audio/video additions from CPE-559 (`wma`/`aiff`/`aif`/`mid`/`midi`/`mpg`/`mpeg`/`3gp`/`mts`/
`m2ts`) — those formats stay uncategorised so their preview remains the correct read-only `info` view. Kept
the **safe** icon-only additions whose preview is handled correctly or falls back cleanly: `psd` (via the
decoded-image provider), `epub`/`mobi`/`pages` (document icon; epub → info preview), `iso`/`dmg`/`cab`/`lz`/
`lzma` (archive icon; iso → archive preview), `appimage`. Updated `filetypes.test.ts` accordingly and added
a guard asserting `mid`/`midi`/`wma`/… stay `unknown`. Full frontend suite re-run: **599 pass / 62 files**
(`provider.test.ts` 19 ✓, `filetypes.test.ts` 30 ✓); `npm run check` 0/0.

## Notes
Process lesson: **run the full suite (not just the touched file's test) before landing** — a pure-data
change can affect other consumers (here the preview provider). Category ≠ preview capability.
