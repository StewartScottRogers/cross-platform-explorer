---
id: CPE-1002
title: "EPIC: File inspection & safety utilities"
type: Task
status: In Progress
priority: Medium
component: Backend
tags: [epic]
created: 2026-07-24
closed:
---

> **Filed + activated 2026-07-24** (workshift, Foreman; ideas from a research sweep — each grep-verified
> non-duplicative against the 78 `cpe-server` modules + 39 epics). An umbrella for small, **pure,
> headless** file-inspection detectors — deterministic algorithms over already-gathered bytes/metadata, no
> GUI logic and no live AI model in the core, no new deps. Each ships its detector; the column/badge/review
> UI is attended.

## Goal
A suite of pure detectors that surface useful facts + hazards about files the explorer doesn't show today:
the true text **encoding + line endings** of a file, **archive expansion ratios** (zip-bomb warning),
**empty-folder** cascades, **orphaned sidecar** files, and **dangling symlinks**. Each is a small,
cargo/vitest-testable core the UI later surfaces as a column, badge, or cleanup view.

## Why
The explorer previews and organises, but doesn't inspect for these common real-world issues (a Latin-1 file
mangled on save, an archive that balloons 1000× on extract, folders full of nothing, `.srt`/`.xmp` sidecars
whose media was deleted, symlinks pointing nowhere). All are pure algorithms over metadata/bytes — ideal
headless slices, and genuinely useful for a cross-platform explorer.

## Child tickets
1. **CPE-1003** — Text **encoding + line-ending** detector (`cpe-server::text_encoding`): `detect_encoding(bytes)
   -> EncodingGuess` (BOM sniff + UTF-8 validity + heuristic) and `detect_line_endings(text) ->
   LineEndingReport`. Pure byte-scan, no deps. *Headless — buildable now.*
2. **CPE-1004** — Archive **expansion-ratio / zip-bomb** score (extend `cpe-server::archive`): read the
   `zip` crate's already-available `compressed_size`, then `expansion_ratio(entries) -> RatioReport`
   flagging dangerous ratios. Pure over listed entry metadata (no extraction), no new deps.
   *Headless — buildable now.*
3. **CPE-1005+** — further pure detectors from the research sweep: empty-folder cascade finder, orphaned-
   sidecar detector, dangling-symlink scanner. Each a small pure core. *Headless.*
4. **UI** — columns / warning badges / cleanup-review surfaces for the above. **GUI/attended.**

## Definition of Done
- Each detector is a pure, cargo/vitest-tested `cpe-server`/`src/lib` function with no new deps and no cost
  when unused; the UI surfaces them (attended).
