---
id: CPE-1361
title: "Make every sample fixture real & substantial (not tiny metadata stubs) for robust preview/GUI testing"
type: Task
status: Backlog
priority: High
component: Multiple
tags: [ready]
epic: CPE-1148
created: 2026-08-06
closed:
---

## Problem

Most `samples/` fixtures are minimal metadata-only stubs — too trivial to exercise the previews robustly
(user-reported). Current sizes: mp3 395 B / flac 193 B / ogg 178 B (silent stubs), mp4 207 B (no real frames),
jpg 195 B / png 75 B / tiff 192 B (2×2 px), zip 182 B (ONE file), wasm **8 B** (magic only), ttf 736 B
(1 glyph), text files ~50–90 B. Only the recently-added dcm/heic/cr2 are substantial.

## Goal

Regenerate every sample so it is **real, valid, and substantial** — enough to make the preview panes and the
gui-smoke walk (CPE-1358) a robust test. Keep them synthetic (no copyrighted media) and, where practical,
deterministic. Tooling available on this machine: **ffmpeg + ffprobe** (on PATH and bundled), **PIL/Pillow**,
Python **stdlib** (wave, zipfile, sqlite3, struct). Update `scripts/gen_samples.py` to produce them (the
source of truth) and regenerate.

### Per-type substance targets
- **Audio (mp3/flac/ogg):** generate a few seconds of REAL audible audio — synth a WAV melody/chord via stdlib
  `wave` (e.g. a short arpeggio, stereo, 44.1 kHz), then `ffmpeg` encode to mp3/flac/ogg **with the metadata
  tags** (Title/Artist/Album/etc. = the existing baseline constants). Tens–hundreds of KB. Verify tags via
  `cpe_server::media_meta::read_all`.
- **Video (mp4):** a real short clip (e.g. `ffmpeg` `testsrc`/a generated animation, a few seconds, with an
  audio track), H.264/AAC, plays in the webview. Verify metadata reads.
- **Images (jpg/png/tiff):** real photos, a few hundred px (e.g. 800×600), via PIL — a recognizable synthetic
  scene, JPEG with EXIF (keep the baseline EXIF tags), PNG, and the decoded-image TIFF. KB-scale.
- **Archive (zip):** a REAL multi-file, multi-**folder** archive (e.g. `docs/readme.md`, `images/logo.png`,
  `data/table.csv`, `src/main.py`, a nested `docs/sub/notes.txt`) with actual content — so drilling in shows
  real structure and inner-file preview has real files to render. (This directly supports the archive-preview
  fix, CPE-1360.)
- **RAR:** the RAR is hand-built (no `rar` encoder available). Make it substantial too — hand-build a RAR4/RAR5
  with several **stored** (uncompressed) real files + a nested folder, mirroring the zip's structure, so
  `rar_entries` lists a real tree. Verify via `cpe_server::rar::rar_entries`.
- **Font (ttf):** a font with MANY glyphs (a real specimen), not a 1-glyph stub. If a permissively-licensed
  font can't be sourced/generated, at minimum expand the synthesized font to a full ASCII glyph set. Verify
  it renders via `thumb_font::render_glyph_sheet`.
- **wasm:** a REAL small module with a couple of exported functions (e.g. `add`, `fib`) — not just the 8-byte
  magic. Hand-assemble the binary or use a tiny fixed module; verify `binary_preview::wasm_info` disassembles
  it to meaningful WAT.
- **sqlite:** several tables with realistic rows (e.g. a `users`/`orders` schema, dozens of rows) so the
  data-grid has real content to page.
- **pdf:** keep the valid multi-page PDF; optionally add a couple more pages / an embedded image so it's a
  richer render. Keep the `/Info` metadata baseline (sample_fixtures.rs asserts it).
- **text (json/csv/tsv/md/py/txt):** realistic multi-line content (dozens of lines), not one-liners.
- **hex blob:** keep `other/blob.pak` (its job is the hex catch-all); can enlarge modestly.

## Acceptance criteria

- Every sample opens correctly via its real backend parser/preview path (audio/video/image/dcm/etc.) — add a
  cargo/vitest check where cheap; the CPE-1358 gui-smoke walk + coverage ratchet stay green.
- `sample_fixtures.rs` still green (update any byte-exact metadata assertions to the new fixtures; keep the
  documented baseline where the test relies on it).
- `scripts/gen_samples.py` regenerates the tree (document which formats use ffmpeg vs stdlib vs PIL). `samples/`
  is committed. `npm run check` + `cargo test -p cpe-server` green.
- Sizes are reasonable (substantial but not bloated — target KB–low-MB, not tens of MB).

## Notes

Pairs with CPE-1360 (archive-inner-file preview) — the substantial multi-folder zip/rar are the fixtures that
fix needs. User asked to "spend the effort" — do it thoroughly. Epic CPE-1148 (QA/testing).
