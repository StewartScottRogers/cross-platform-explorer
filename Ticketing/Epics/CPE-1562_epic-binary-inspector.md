---
id: CPE-1562
title: "EPIC: Binary Inspector — read-only headers/sections/symbols/imports/resources + disassembly + .NET metadata"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
created: 2026-08-10
closed:
---

> Child of **CPE-1561 (Binary Studio program)**. Dormant brief — not decomposed until activated. **Recommended first build** of the program.

## Why
The safest, highest-value, most license-clean slice: read-only inspection is mostly pure-Rust, builds on what
already ships, needs no heavy sidecar, and is fully headless-verifiable. It's also the substrate every
decompile epic renders into.

## Goal
For an executable selected in the middle pane, a rich read-only inspector view: format + arch (extend
`bin_arch.rs`), headers/sections, imports/exports, symbols, embedded resources, and a real **disassembly**
view. Plus a **.NET/CLR metadata reader** (assembly manifest, referenced assemblies, type/method tables) so
managed binaries are legible before the full decompile epic lands.

## Rough slices (just-in-time when activated)
- Extend `crates/server/src/binary_preview.rs`: full section/import/export/symbol/resource extraction via goblin
  (already a dep) for PE/ELF/Mach-O; DTOs in `cpe-server::model`; skip-on-error; streamed for large tables.
- **Disassembly**: add `iced-x86` (pure-Rust, MIT) for x86/x64; surface a capped, streamed disasm listing. Consider
  ARM later.
- **.NET metadata**: parse CLR metadata tables (assembly/manifest/typedef/methoddef) — pure-Rust CIL reader (evaluate
  crates vs a small hand-rolled reader; the spike CPE-1561/Epic-0 informs this).
- Frontend: a Binary Inspector preview provider plugged into the existing preview seam (CPE-724 shape) with a
  tabbed view (Overview / Sections / Imports / Symbols / Disasm / .NET metadata). Docs per CPE-579.

## Notes
No heavy sidecar; in-process pure-Rust. Fuzz the new parsers (parser-fuzz discipline). PURPOSE tiebreaker: disasm
tables must stream/cap so a huge binary can't stall the pane. Depends on Epic-0 spike only for the .NET-metadata
crate choice.
