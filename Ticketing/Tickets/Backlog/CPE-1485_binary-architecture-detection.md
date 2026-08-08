---
id: CPE-1485
title: "Binary architecture detection (ELF/PE/Mach-O CPU arch) — extend the file-type detector"
type: Feature
status: Backlog
priority: Low
component: Backend
tags: [ready]
epic: CPE-1000
created: 2026-08-08
---
## What (a cheap, headless-buildable win from the superfile reference pass — [[superfile-pm-reference]])
superfile added "binary architecture detection for ELF, PE, and Mach-O" (its changelog). CPE's magic-byte
type detector (epic CPE-1000 / `crates/server` file-type module + CPE-1002 inspection) identifies executable
*formats* but not the target **CPU architecture**. Add that: for a detected ELF / PE / Mach-O, parse the small
fixed header fields that carry the machine type and report a normalized arch string (e.g. `x86-64`, `x86`,
`ARM64`, `ARM`, `RISC-V`, plus 32/64-bit and endianness where relevant; for Mach-O universal/fat binaries,
list the contained slices).

## Where / how
- Extend the existing file-type detector in `crates/server` (the CPE-1000/1002 module — grep for the
  magic-byte/type-detection code). Read only the **header** (bounded read — a few hundred bytes; never the
  whole file), following the existing bounded-read + resource-exhaustion conventions.
  - **ELF**: `e_ident[EI_CLASS]` (32/64), `[EI_DATA]` (endian), `e_machine` (arch).
  - **PE**: the COFF header `Machine` field (via the `PE\0\0` offset from the DOS stub).
  - **Mach-O**: `cputype`/`cpusubtype` in the mach header; handle fat/universal (`0xCAFEBABE`) by listing arch slices.
- **No new Cargo deps** (lean-core): these are a handful of byte-offset reads + a match table; hand-roll like
  the other detectors, don't pull a binary-parsing crate unless it's already in the tree.
- Surface it via the existing inspection/metadata path — a field the Properties/inspection panel or a metadata
  column can show (align with CPE-707 columns / CPE-1002 inspection; a later GUI ticket wires the column).

## Verify (headless)
`cargo test` with small hand-built fixtures: a known x86-64 ELF, an ARM64 ELF, a PE (Machine=0x8664 and 0x14c),
a Mach-O (thin + a fat/universal), an endianness case, and non-binary/garbage → `None`/graceful (no panic,
bounded read). `cargo clippy --all-targets -D warnings` both feature modes.

## Effort
Small — pure logic + a match table + fixtures, same pattern as the shipped CPE-1000/1002 detectors. Backend-only,
fully headless-verifiable. Good workshift-batch fodder.
