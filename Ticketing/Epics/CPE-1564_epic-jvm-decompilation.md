---
id: CPE-1564
title: "EPIC: JVM decompilation — view decompiled Java from .class/.jar (CFR/Procyon sidecar)"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic]
created: 2026-08-10
closed:
---

> Child of **CPE-1561 (Binary Studio)**. Dormant brief. Parallel in shape to the .NET epic (CPE-1563).

## Why
JVM bytecode (.class/.jar) decompiles to high-fidelity Java via **CFR** or **Procyon** (permissive licenses).
Same value story as .NET for the Java/Kotlin ecosystem.

## Goal
For a selected `.class`/`.jar`, show **decompiled Java** with a class/member tree, via an opt-in on-demand JVM-based
sidecar. Read-only first.

## Rough slices (just-in-time)
- CFR-vs-Procyon choice (spike) + a JVM sidecar behind the versioned contract, on-demand via the signed catalog.
- Sidecar command: decompile-class/jar → { Java per class, member tree }; streamed; skip-on-error per entry in a jar.
- Host bridge + frontend tab in the Binary Inspector view.
- Headless fixture-decompile assertion; docs per CPE-579.

## Notes
Read-only; rebuild is CPE-1566 (javac/ASM). Requires a JRE for the sidecar (bundle vs require — spike decides).
Depends on CPE-1562 + Epic-0 spike. Low priority relative to .NET (the user's stated focus).
