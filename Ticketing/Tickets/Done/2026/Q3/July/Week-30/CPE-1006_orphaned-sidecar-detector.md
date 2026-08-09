---
id: CPE-1006
title: Orphaned-sidecar-file detector
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-1002
---

## Summary
A pure, deterministic **orphaned-sidecar-file detector** for `cpe-server` (CPE-1002 "File inspection &
safety utilities"). A sidecar is a companion file (subtitle, XMP/NFO metadata, camera thumbnail) named
after a primary file; it's "orphaned" when that primary file is missing. Operates on a caller-supplied,
already-gathered listing of one directory — no filesystem access, no AI, no new deps. Sprint (Foreman)
background slice, 2026-07-24.

## Design
- `FileEntry { name, stem, ext }` — one file's name, its stem (name without final extension), and its
  lowercased dot-free extension. All entries passed together are assumed to be in the same directory.
- `SidecarRule { sidecar_ext, primary_exts }` — a sidecar extension and the primary extensions it
  companions (lowercased, no dot).
- `default_rules() -> Vec<SidecarRule>` — the built-in rule set:
  - `srt` / `sub` / `ass` (subtitle tracks) → video (`mp4`/`mkv`/`avi`/`mov`/`webm`)
  - `xmp` (Adobe/Lightroom + camera-raw sidecar metadata) → image/raw (`jpg`/`jpeg`/`png`/`tiff`/`cr2`/
    `nef`/`arw`/`dng`)
  - `nfo` (Kodi/Plex-style scraper metadata) → video (same set as above)
  - `thm` (camera-generated thumbnail) → video **and** image (cameras emit `.thm` for both photo and
    video captures)
- `find_orphans(entries, rules) -> Vec<String>` — for each entry whose `ext` matches a rule's
  `sidecar_ext` (case-insensitive), reports its `name` when no *other* entry shares its `stem`
  (case-insensitive) with an `ext` in that rule's `primary_exts` (case-insensitive). Non-sidecar files,
  and sidecars with a present primary, are never reported. Output preserves input order (deterministic).
- Pure std, zero new deps — same shape as `crate::organize`/`crate::duplicates`.

## Acceptance Criteria
- [x] `movie.mp4` + `movie.srt` → not orphaned (primary present); `deleted.srt` alone → orphaned.
- [x] `photo.jpg` + `photo.xmp` → kept; `orphan.xmp` alone → orphaned.
- [x] Stem matching is case-insensitive (`Movie.MP4` + `movie.srt` → kept).
- [x] A sidecar whose stem matches a file of the wrong primary type (`notes.srt` + `notes.txt`) is still
      reported as orphaned.
- [x] A non-sidecar file (`readme.txt`) is never reported.
- [x] Output order is deterministic (input order) with multiple orphans.
- [x] `cargo test --lib orphan_sidecars` passes (10 tests); `cargo clippy --all-targets -- -D warnings`
      and `cargo clippy --all-targets --features index -- -D warnings` both clean; no new deps.

## Work Log
- 2026-07-24 (sprint, Foreman): Implemented `crates/server/src/orphan_sidecars.rs` +
  `pub mod orphan_sidecars;` in `lib.rs`. Default rule set as above (srt/sub/ass→video, xmp→image/raw,
  nfo→video, thm→video+image) — chosen from the ticket's examples plus the common real-world sidecar
  formats (Kodi/Plex `.nfo`, camera `.thm`) that share the same "companion of a media primary" shape.
  10 unit tests cover the ticket's required scenarios plus rule-set coverage. `cargo test --lib
  orphan_sidecars`: 10/10 pass. `cargo clippy --all-targets -- -D warnings`: clean. `cargo clippy
  --all-targets --features index -- -D warnings`: clean. Branch `cpe-1006-orphaned-sidecar-detector`,
  PR opened for Foreman review (not merged).
