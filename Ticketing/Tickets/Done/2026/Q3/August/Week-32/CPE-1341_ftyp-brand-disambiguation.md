---
id: CPE-1341
title: "file_type: read the ftyp brand so .mov/.heic/.avif aren't false-flagged as MP4 mismatches"
type: Bug
status: Done
priority: Medium
component: cpe-server
tags: [ready]
epic: CPE-1000
created: 2026-08-05
closed: 2026-08-05
---

## Problem

`crates/server/src/file_type.rs::detect_type` maps **every** ISO-BMFF file (a 4-byte box
size followed by `ftyp` at offset 4) to `FileType::Mp4`, whose `extensions()` are only
`["mp4", "m4a", "m4v"]`. But the ISO base-media container backs many more formats that are
distinguished by the **major brand** (the 4 bytes at offset 8, right after `ftyp`):

- `qt  ` → QuickTime `.mov`
- `heic` / `heix` / `heif` / `mif1` / `msf1` → `.heic` / `.heif`
- `avif` / `avis` → `.avif`
- `3gp*` (`3gp4`, `3gp5`, …) → `.3gp`
- `M4A ` → `.m4a`, `M4V ` → `.m4v`, `M4B ` → `.m4b`, `isom`/`mp42`/`mp41`/`dash` → `.mp4`

Because they all currently resolve to `Mp4`, a genuine `.mov`, `.heic`, `.avif`, or `.3gp`
file is reported as a **content/extension mismatch** — the exact false positive the epic's
open questions warn against. This is a live correctness bug in the shipped `mismatch()` /
true-type surface.

## Fix

Read the major-brand tag at offset 8 and resolve the ftyp container to the right `FileType`.
Add the variants the brands map to and their canonical extensions:

- `FileType::Mov` → `["mov", "qt"]` (label "QuickTime video")
- `FileType::Heic` → `["heic", "heif", "hif"]` (label "HEIC image")
- `FileType::Avif` → `["avif"]` (label "AVIF image")
- `FileType::ThreeGpp` → `["3gp", "3g2"]` (label "3GPP video")
- keep `FileType::Mp4` for the mp4/m4a/m4v/isom/dash family (unrecognised-but-present ftyp
  brands should still fall back to `Mp4`, the safe generic — never regress a real `.mp4` to
  "unknown").

Brand matching is a prefix/table check on the 4 brand bytes; be tolerant of the trailing
space padding (`qt  `, `M4A `). Keep it pure, bounds-checked, no new deps.

## Acceptance criteria

- A `.mov` (`ftyp` brand `qt  `), `.heic` (`heic`), `.avif` (`avif`), `.3gp` (`3gp4`) file
  each detects as its own type and `mismatch(bytes, "mov"|"heic"|"avif"|"3gp")` is `None`.
- A real `.mp4` (`ftypisom` / `ftypmp42`) still detects as `Mp4`; an ftyp with an
  unrecognised brand still falls back to `Mp4` (no regression to unknown).
- A `.mov` renamed to `.jpg` still mismatches (detected `Mov`).
- New per-format unit tests; existing `detects_mp4_by_ftyp_tag_at_offset_4` still passes.
- `cargo test` + `clippy --all-targets -D warnings` (both feature modes) green. No new deps.

## Notes

Pure `cpe-server` change; headless, cargo-testable. If any of the new `FileType` variants
touch a `specta::Type`-exported struct, regenerate `bindings.gen.ts` (they should not — this
enum is internal to the sniffer). Feeds epic CPE-1000.

## Work Log
- 2026-08-05 (sprint): Implemented in PR #637 (squash-merged to main as 3425e136). Worker(sonnet); independent Reviewer APPROVE + UAT PASS; all backend/server/sidecar/frontend CI green on 3 OS. GUI-smoke cancelled twice (concurrency-group supersede, not a real failure) — non-blocking on a pure-backend diff; main is unprotected so gauntlet+authoritative-CI is the gate. No new deps; bindings.gen.ts unchanged.
