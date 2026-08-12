---
id: CPE-1566
title: "EPIC: Edit & rebuild — recompile/patch edited binaries per family (.NET/JVM faithful; native = reassemble/patch)"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic, big-design]
created: 2026-08-10
closed:
---

> Child of **CPE-1561 (Binary Studio)**. Dormant brief. The read-write arm. Each family depends on its decompile epic.

## Why
The "recompile an executable" half of the user's ask. Feasibility is **strongly family-dependent** — this epic
exists to deliver it honestly where it's real and refuse it where it isn't.

## Goal (per family — decompose per family when activated)
- **.NET (faithful):** edit IL and reassemble via **Cecil/Mono.Cecil** (true modify-and-save, the reliable path);
  optionally recompile edited decompiled C# via **Roslyn** (`csc`) — lossier, flagged as such.
- **JVM (faithful):** patch bytecode via **ASM**, or recompile edited Java via **javac**.
- **Native (honest boundary):** **reassemble edited assembly** / **binary patch** only. Recompiling decompiled
  pseudo-C back into the same program is **not** offered — the UI states this plainly. No false promise.

## Rough slices (per family, just-in-time)
- .NET rebuild (Cecil sidecar; edit-IL round-trip test on a fixture) — first, matches the primary ask.
- JVM rebuild (ASM/javac sidecar).
- Native patch/reassemble (edit disasm → reassemble; byte-patch with checksum/relocation care).
- Safety: **recompile ≠ run** — never auto-execute output; write to a user-chosen path; sandbox toolchain; confirm overwrite.
- Docs per CPE-579.

## Notes
`big-design` — highest-risk arm. Depends on CPE-1563 (.NET) / CPE-1564 (JVM) / CPE-1565+1562 (native) respectively.
Security review mandatory (writes executables). PURPOSE: sidecar-delivered, opt-in.
