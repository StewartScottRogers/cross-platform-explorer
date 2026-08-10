---
id: CPE-1563
title: "EPIC: .NET/CLR decompilation — view decompiled C# + IL (ILSpy-based sidecar)"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
created: 2026-08-10
closed:
---

> Child of **CPE-1561 (Binary Studio)**. Dormant brief. The user's primary ask (".NET built executables"). High-fidelity, read-only first.

## Why
.NET assemblies (PE + CLR metadata) decompile back to near-original C#/VB/F# — the highest-fidelity decompile
target. ILSpy's engine (`ICSharpCode.Decompiler`, **MIT**) / `ilspycmd` is the reference-quality tool.

## Goal
For a selected managed assembly, show **decompiled C#** and the underlying **IL**, with a type/member tree,
delivered via an opt-in, on-demand **sidecar** (keeps the lean core clean per PURPOSE.md).

## Rough slices (just-in-time)
- Delivery decision (spike-confirmed): an `ilspycmd`/ILSpy-lib sidecar behind the versioned contract
  (`crates/contract`), downloaded on demand via the signed catalog; clean "engine not installed → install?" path.
- Sidecar command surface: decompile-assembly → { C# per type, IL, member tree }; bounded/streamed output.
- Host bridge + frontend: a "Decompile (C#)" / "View IL" tab in the Binary Inspector (CPE-1562) view.
- Headless verification: decompile a known fixture assembly, assert stable output; cross-platform where .NET runtime present.
- Docs per CPE-579.

## Notes
Read-only here; **edit + rebuild is CPE-1566**. License: ILSpy MIT — clean. Requires a .NET runtime for the
sidecar (bundle vs require — spike decides). Depends on CPE-1562 (renders into the inspector) + Epic-0 spike.
