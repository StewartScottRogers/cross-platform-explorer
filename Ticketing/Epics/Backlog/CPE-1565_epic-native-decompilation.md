---
id: CPE-1565
title: "EPIC: Native decompilation (best-effort) — approximate pseudo-C via Ghidra/RetDec sidecar"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic, big-design]
created: 2026-08-10
closed:
---

> Child of **CPE-1561 (Binary Studio)**. Dormant brief. The heaviest, most-gated epic — build last.

## Why
Native PE/ELF/Mach-O (C/C++/Rust/Go) can be **disassembled reliably** (that's CPE-1562) and **decompiled to
approximate, lossy pseudo-C** via **Ghidra** (Apache-2.0, headless mode) or **RetDec** (MIT). Honest framing:
this is best-effort reverse-engineering, **not** a faithful source recovery, and it does **not** enable native
recompile (see CPE-1566's honest boundary).

## Goal
For a selected native binary, offer an opt-in best-effort **pseudo-C** view alongside the reliable disassembly,
clearly labelled as approximate.

## Rough slices (just-in-time)
- Engine choice (spike): Ghidra headless analyzer vs RetDec — size, license, cross-platform, output quality.
- A heavyweight sidecar behind the versioned contract, on-demand via the signed catalog; explicit size warning at install.
- Sidecar command: analyze-function/binary → pseudo-C (per-function, streamed, capped); host bridge + frontend tab.
- Prominent "approximate / best-effort" labelling in the UI.
- Docs per CPE-579.

## Notes
`big-design` — Ghidra is a large Java app; RetDec is also heavy. PURPOSE tiebreaker makes on-demand sidecar
delivery mandatory. Depends on CPE-1562 + Epic-0 spike. Lowest priority; the value ceiling is inspection +
disassembly for most native use.
