---
id: CPE-1561
title: "EPIC (PROGRAM): Binary Studio — inspect · decompile · rebuild executables from the file pane"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic, program]
created: 2026-08-10
closed:
---

> **Approved by the user 2026-08-10.** Parent/umbrella program epic. Child epics (CPE-1562…1567) are
> dormant briefs decomposed just-in-time; Epic 0 is a research spike (dispatched, not a buildable epic).

## Vision
For an executable or compiled artifact **selected in the middle pane**, let the user: (1) **view/inspect**
its internals, (2) **decompile** it to readable source/IL, (3) **edit + recompile** it back to a working
binary, and (4) more broadly **compile anything in the pane**. Delivered so the *plain explorer stays lean* —
heavy engines are opt-in, sidecar-delivered, on-demand.

## Honest fidelity reality (drives staging)
| Family | View/Disasm | Decompile | Edit+Recompile round-trip |
|---|---|---|---|
| **.NET/CLR** | ✅ | ✅ near-original (ILSpy, MIT) | ✅ genuine (Cecil IL / Roslyn C#) |
| **JVM** (.class/.jar) | ✅ | ✅ high (CFR/Procyon) | ✅ (javac / ASM) |
| **Python** (.pyc) | ✅ | ⚠️ version-limited | ⚠️ (it's source) |
| **WASM** | ✅ (wasm→WAT already ships) | ⚠️ partial | ⚠️ wat2wasm |
| **Native** (PE/ELF/Mach-O) | ✅ disasm reliable | ⚠️ approximate pseudo-C (Ghidra/RetDec) | ❌ decompiled-C **cannot** faithfully recompile — honest offering = reassemble/patch |

**Sweet spot = .NET + JVM** (full view→decompile→rebuild works). **Native recompile of decompiled C is not a
real capability** — we ship disassemble + reassemble/patch there and say so plainly.

## What already exists (extend, don't rebuild)
- `crates/server/src/binary_preview.rs` — PE summary via **goblin** (already a dep), **wasm→WAT** (CPE-216).
- `crates/server/src/bin_arch.rs` — ELF/PE/Mach-O architecture detection.
- Code-intelligence preview seam (CPE-724): `code_intel.rs`/`code_outline.rs`/`code_folds.rs`.
- Sidecar platform (`crates/contract`, `crates/net`) + signed auto-update catalog — the delivery vehicle for heavy engines.

## Constraints (non-negotiable)
- **PURPOSE.md fast/small/predictable:** decompiler engines are big → opt-in, **sidecar-delivered, downloaded on
  demand**; the lean core is untouched when the feature is off.
- **License-clean only:** ILSpy (MIT), CFR/Procyon (permissive), Ghidra (Apache-2.0), goblin/iced-x86 (MIT).
  **Exclude commercial** (IDA/Hex-Rays). The spike locks the per-engine matrix.
- **Security:** untrusted-binary parsing = attack surface → adversarial fuzzing per our parser-fuzz discipline;
  **recompile ≠ run** (never auto-execute a built/patched binary); sandbox all toolchain invocation.
- **Cross-platform:** pure-Rust parts in-process everywhere; .NET/JVM/Ghidra engines as sidecars where the runtime exists.

## Child epics (decompose just-in-time)
- **Epic 0 (research spike)** — license-clean engine matrix + delivery-architecture decision + refined roadmap. *Run first (Researcher), not a buildable epic.*
- **CPE-1562 — Binary Inspector** (read-only, universal, mostly pure-Rust, headless). *Recommended FIRST build.*
- **CPE-1563 — .NET/CLR decompilation** (ILSpy sidecar → view C# + IL).
- **CPE-1564 — JVM decompilation** (CFR/Procyon sidecar → view Java).
- **CPE-1565 — Native decompilation, best-effort** (Ghidra/RetDec sidecar → approximate pseudo-C).
- **CPE-1566 — Edit & rebuild** (read-write arm; per-family: .NET Cecil/Roslyn, JVM javac/ASM, native reassemble/patch).
- **CPE-1567 — Compile anything in the pane** (toolchain detection + sandboxed compile + diagnostics).

## Epic-0 spike findings (2026-08-10) — LOCKED decisions
See Library `binary-studio-engines-delivery-2026-08-10`. Key rulings:
- **Delivery:** ADR-0001 forbids downloaded sidecar *binaries* (the signed catalog ships JSON only). **Do NOT**
  ship engines as catalog bundles. **Reuse the AI Console detect/install-RECIPE pattern** (`sidecar/ai-console/src/agents.rs`
  + `agents/*.json`): catalog ships a signed *recipe* + SHA-256s; the engine is fetched by its official installer or a
  checksum-pinned self-contained bundle. **Author an ADR note** recording this (no ADR amendment needed).
- **Seam:** engine sidecars speak `sidecar-contract` (`sidecar/contract/src/lib.rs`), NOT `crates/contract`.
- **Engines (license-clean):** .NET = ILSpy/ilspycmd (MIT, prefer self-contained ~18MB) + Mono.Cecil (MIT) for rebuild;
  JVM = CFR (MIT); native disasm = iced-x86 (MIT, x86/x64, in-process) + yaxpeax-arm (0BSD); native decompile = RetDec
  (MIT) or Ghidra (Apache-2.0). **`dotnetdll` is GPL-3 → excluded in-process**; commercial (IDA) excluded. GPL engines
  only ever as arm's-length separate processes.
- **CPE-1562 is all in-process pure-Rust** (goblin already a dep, hand-rolled ECMA-335 CLR reader, iced-x86) → ships
  in the lean core path; heavy decompile engines live behind the `sidecar-platform` cargo feature (OFF by default).
- **Fuzz:** the hand-rolled CLR reader is the top adversarial-fuzz priority (coded-index/offset parsing).

## Sequencing
Epic 0 spike → CPE-1562 (Inspector) → CPE-1563 (.NET, the primary ask) → CPE-1564 (JVM) in parallel → CPE-1565
(native) later/most-gated → CPE-1566 (rebuild, per-family, each after its decompile epic) → CPE-1567 (general
build) as a later, possibly-separate program.
