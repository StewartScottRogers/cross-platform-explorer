---
id: CPE-1342
title: "file_type: add common missing magic signatures (tar, psd, cab, icns, ar/deb, aiff, midi, flv, cur, lz4, lzip)"
type: Task
status: Done
priority: Medium
component: cpe-server
tags: [ready]
epic: CPE-1000
created: 2026-08-05
closed: 2026-08-05
---

## Problem

`crates/server/src/file_type.rs` covers a good curated set, but several **common** formats
are still undetected, so a file of one of these types renamed to a lying extension is neither
surfaced in the true-type column nor caught by `mismatch()`. Each below is a real, widely-seen
format with an unambiguous (or safely-scoped) leading/known-offset signature and needs **no new
dependency**.

## Add these signatures

| Type | Signature | Canonical exts | Label |
|------|-----------|----------------|-------|
| `Tar` | `ustar` at **offset 257** (both `ustar\0` POSIX and `ustar  ` GNU variants) | `tar` | "TAR archive" |
| `Psd` | `8BPS` at 0 | `psd`, `psb` | "Photoshop document" |
| `Cab` | `MSCF` at 0 | `cab` | "Windows Cabinet archive" |
| `Icns` | `icns` at 0 | `icns` | "Apple icon image" |
| `Ar` | `!<arch>\n` at 0 | `a`, `ar`, `deb`, `lib` | "Unix archive / Debian package" |
| `Aiff` | `FORM` at 0 **and** `AIFF` or `AIFC` at offset 8 (mirror the existing RIFF-at-offset-8 pattern) | `aif`, `aiff`, `aifc` | "AIFF audio" |
| `Midi` | `MThd` at 0 | `mid`, `midi` | "MIDI sequence" |
| `Flv` | `FLV` at 0 | `flv` | "Flash video" |
| `Cur` | `00 00 02 00` at 0 (ICONDIR idType=2 — sibling of the existing `Ico` `00 00 01 00`) | `cur` | "Windows cursor" |
| `Lz4` | `04 22 4D 18` at 0 | `lz4` | "LZ4 archive" |
| `Lzip` | `LZIP` at 0 | `lz` | "lzip archive" |

## Ordering / collision notes (respect the module's existing ordering discipline)

- `Cur` (`00 00 02 00`) sits right next to `Ico` (`00 00 01 00`) and `TrueType`
  (`00 01 00 00`) — full-length exact matches, no shadowing, but add a test asserting the
  three don't collide (like the existing ICO/TrueType test).
- `Aiff` reuses the `FORM`+offset-8 idea; `FORM` alone is also the IFF container prefix, so
  gate strictly on the `AIFF`/`AIFC` sub-tag at offset 8 (don't detect bare `FORM`).
- `Ar`'s `!<arch>\n` also underlies `.deb`; list `deb` among its extensions so a Debian
  package isn't false-flagged (mirrors the ZIP-container reasoning).
- Keep every check bounds-checked via `matches_at`; `Tar`'s offset-257 check must not panic
  on short input (it won't — `matches_at` is length-guarded).

## Acceptance criteria

- `detect_type` recognises each new format from a minimal fixture; `mismatch()` flags each
  when renamed to a foreign extension and returns `None` under its own extension(s).
- Short/empty/truncated input still never panics (extend the existing short-input test loop
  if useful).
- `label()` / `extensions()` sanity test extended to the new variants.
- `cargo test` + `clippy --all-targets -D warnings` (both feature modes) green. **No new deps.**

## Notes

Pure `cpe-server` change; headless, cargo-testable. Feeds epic CPE-1000. Touches the same
file as CPE-1341 (ftyp brands) — sequence them on one worker to avoid a merge collision.

## Work Log
- 2026-08-05 (sprint): Implemented in PR #637 (squash-merged to main as 3425e136). Worker(sonnet); independent Reviewer APPROVE + UAT PASS; all backend/server/sidecar/frontend CI green on 3 OS. GUI-smoke cancelled twice (concurrency-group supersede, not a real failure) — non-blocking on a pure-backend diff; main is unprotected so gauntlet+authoritative-CI is the gate. No new deps; bindings.gen.ts unchanged.
