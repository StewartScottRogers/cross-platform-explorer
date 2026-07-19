---
id: CPE-719
title: "EPIC: Hex & binary inspector"
type: Task
status: Proposed
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
