---
title: "Binary Studio (CPE-1561): license-clean decompiler engines + how to deliver them without violating ADR-0001"
slug: binary-studio-engines-delivery-2026-08-10
date: 2026-08-10
status: current
tags: [binary-studio, cpe-1561, cpe-1562, cpe-1563, decompile, ilspy, cfr, ghidra, iced-x86, sidecar, adr-0001, install-recipe, licensing, ecma-335, pm-reference]
---

## The finding that drives everything
**ADR-0001 (`docs/adr/0001-sidecar-platform.md:53`) forbids downloaded sidecar BINARIES — the signed catalog
ships signed JSON only** (`sidecar/host/src/catalog.rs` verifies/gates JSON manifests). So "heavy engine as an
on-demand signed-catalog bundle" does NOT exist and would need an ADR amendment. **Reuse instead the AI Console's
detect/install-RECIPE pattern** (`sidecar/ai-console/src/agents.rs`, `agents/*.json`): the catalog ships a signed
*recipe* (JSON) + expected SHA-256s; the actual engine is fetched by its official installer / a checksum-pinned
self-contained bundle. No ADR change, reuses the ed25519 trust chain. **Author an ADR note recording this.**

## Delivery tiers (each mapped to existing machinery)
- **Pure-Rust, small, license-clean → in-process in `cpe-server`** (preview seam CPE-724, `model` DTOs, STREAMING.md).
  This is ALL of CPE-1562 (inspector): goblin extraction (goblin already a dep), hand-rolled CLR reader, iced-x86 disasm.
- **.NET/JVM/native decompile engines → sidecar behind `sidecar-contract`** (`sidecar/contract/src/lib.rs`:
  `Request{method,params}`/`Response`/`Event::Progress`; NOT `crates/contract` which is the GUI↔Server boundary),
  engine delivered via the detect/install-recipe pattern. Crash-isolated (malformed binary kills only the sidecar).
- **`sidecar-platform` cargo feature is OFF by default** → mode off = plain explorer byte-for-byte unchanged (PURPOSE tiebreaker + delete-test).

## Engine matrix (license-clean picks)
- **.NET decompile (CPE-1563, primary ask):** **ILSpy `ICSharpCode.Decompiler`/`ilspycmd` (MIT)** — prefer a
  **self-contained publish** (~18MB trimmed, no user .NET needed) fetched by recipe. Rebuild (CPE-1566) = **Mono.Cecil (MIT)** IL round-trip (reliable) / Roslyn C# (lossy).
- **.NET metadata READER for CPE-1562:** **hand-roll an ECMA-335 reader** — the only mature pure-Rust CLR crate
  `dotnetdll` is **GPL-3 → EXCLUDE from in-process use**. goblin already finds the PE + CLI header (data-dir #14);
  parse the `#~` stream tables (Assembly/TypeDef/MethodDef). Small, license-clean. **Highest fuzz priority** (variable-width coded-index encodings = slice-at-offset/overflow territory).
- **JVM (CPE-1564):** **CFR (MIT)** — single ~2-3MB jar, best modern-Java output. Rebuild = ASM (BSD-3)/javac. Needs a JRE (detect or jlink bundle).
- **Native disasm (CPE-1562, reliable):** **iced-x86 (MIT, x86/x64, ~250KB, in-process)** now; **yaxpeax-arm (0BSD, ARM)** next; capstone (BSD-3, C build) only for exotic arches.
- **Native decompile (CPE-1565, heaviest/last):** RetDec (MIT, native, no JDK — lower friction) or Ghidra (Apache-2.0, best quality, ~400MB + JDK). On-demand, "approximate" label.
- **WASM:** already ships wasm→WAT (wasmprinter). **Python .pyc:** decompilers are GPL + version-limited → separate-process sidecar only, low priority.
- **GPL rule:** GPL engines OK ONLY as arm's-length separate processes over the contract (mere aggregation); NEVER linked in-process as a Cargo dep. **Commercial (IDA/Hex-Rays) excluded.**

## Recompile honesty (confirmed)
.NET = Cecil IL round-trip reliable (Roslyn C# lossy); JVM = ASM patch reliable (javac lossy); **native = reassemble/patch ONLY — decompiled-C recompile is NOT real** (state in UI). Sweet spot = .NET + JVM.

## Security
Fuzz new untrusted parsers (battery: `crates/server/tests/parser_panic_safety.rs`): the hand-rolled CLR reader
(top priority), extended goblin iteration (bounds/skip-on-error, keep list_dir skip-not-fail), disasm streaming
loop (infinite-loop/hang class). recompile≠run (write to chosen path, confirm overwrite, reuse backup.rs);
sandbox toolchain (CPE-1566/67); atomic temp-write+rename for output; sidecar crash-isolation is itself a security feature.

## Build order
CPE-1562 FIRST (in-process, headless): smallest slice = promote `binary_preview.rs::pe_info` to a structured
`cpe-server::model` DTO + ELF/Mach-O parity via goblin (Overview/Sections/Imports/Exports/Symbols, streamed,
skip-on-error) → then disasm tab (iced-x86) → .NET metadata tab (hand-rolled) → frontend tabbed provider.
Then CPE-1563 (.NET decompile sidecar + recipe). Then 1564 (JVM) / 1565 (native) / 1566 (rebuild) / 1567 (compile-anything).
See [[untrusted-parser-fuzz-sweep-2026-08-07]], [[cpe-server-logic-audited]].
