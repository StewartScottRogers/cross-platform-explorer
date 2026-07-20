---
id: CPE-719
title: "EPIC: Hex & binary inspector"
type: Task
status: In Progress
priority: Low
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
A proper hex/binary viewer for the preview pane: a paged hex+ASCII dump for arbitrarily large files, a live
data inspector interpreting the bytes under the cursor as int8/16/32/64, float, string, and timestamp in
both endiannesses, plus magic-byte format-signature detection.

## Why
Nothing binary-aware exists today. Developers, reverse-engineers, and forensics users expect a hex view; it
also complements the "what is this file really?" question the app can't currently answer.

## Rough scope (areas, not child tickets)
- Streaming byte-range reads in Rust for arbitrarily large files.
- A virtualized hex grid (offset / hex / ASCII) in the preview pane.
- A data-inspector panel decoding the selection across types and endiannesses.
- Magic-byte signature detection (PNG/ELF/ZIP/...) with optional structure-template overlays.

## Open questions (resolve at activation)
- Preview provider integration vs. a dedicated inspector mode.
- How far to take structure templates (nice-to-have vs. core).
- Edit-in-hex now or read-only v1?

## Definition of Done
- Any file opens in a paged hex+ASCII view that handles very large files without loading them fully.
- The data inspector decodes bytes under the cursor across integer/float/string/timestamp types.
- Common formats are identified by signature; the viewer is read-only-safe.

## Work Log
2026-07-19 (nightshift, 23:54 MST) — Activated. Open questions resolved (autonomous best-guess, PM
delegated): **preview-provider integration** (a hex mode inside the existing preview pane, not a separate
inspector app); **read-only v1** (no edit-in-hex; DoD says read-only-safe); **structure templates
deferred** to a follow-up (nice-to-have). Foundation-first so the pure logic lands headless before any GUI.

## Child tickets
1. **CPE-770** — Pure hex-dump formatting (`src/lib/hexdump.ts`): a byte range → offset/hex/ASCII rows +
   magic-byte signature detection (PNG/ELF/ZIP/PDF/GZIP/…). Unit-tested. **Foundation, headless.**
2. **CPE-771** — Pure data-inspector decoders (`src/lib/hexinspect.ts`): a byte offset → int8/16/32/64,
   uint, float32/64, ASCII/UTF-16 string, and Unix/Windows timestamp, in both endiannesses. Unit-tested.
   **Headless.** *(independent of 770)*
3. **CPE-772** — Backend streaming byte-range read command `read_file_range(path, offset, len)` for
   arbitrarily large files (async + spawn_blocking; cargo-tested). **Backend, CI-verified.**
4. **CPE-773** — HexView preview provider: a virtualized offset/hex/ASCII grid + data-inspector panel +
   signature badge, wired into the preview pane, consuming 770/771/772. **Attended GUI.** *(prereq: 770–772)*
