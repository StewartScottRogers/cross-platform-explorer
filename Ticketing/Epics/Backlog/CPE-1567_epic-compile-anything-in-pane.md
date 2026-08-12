---
id: CPE-1567
title: "EPIC: Compile anything in the pane — toolchain detection + sandboxed build + diagnostics"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic, big-design]
created: 2026-08-10
closed:
---

> Child of **CPE-1561 (Binary Studio)**. Dormant brief. The broadest arm — possibly its own program.

## Why
The generalization of the user's "compile anything found in the middle pane": given source (or edited decompiled
output) selected in the pane, detect the language/toolchain and build it, surfacing diagnostics — a lightweight
build front-end inside the explorer.

## Goal
Right-click a source file / folder → detect toolchain (rustc/cargo, gcc/clang, csc/dotnet, javac, tsc, python,
go, …) → build in a sandbox → stream compiler diagnostics into a results pane → produce the artifact in the pane.

## Rough slices (just-in-time)
- Toolchain detection + capability descriptor (which compilers are present on this machine; clean "not installed" UX).
- Sandboxed invocation harness (never auto-run the output; resource/time caps; working-dir isolation).
- Diagnostics parsing/streaming into a results surface (reuse code-intel/preview seams where possible).
- Per-language adapters, added incrementally, starting with the most common.
- Docs per CPE-579.

## Notes
`big-design` — largest scope; overlaps the Terminal Dock (CPE-714) and scriptable-actions (CPE-739) — reuse, don't
duplicate. Sequence LAST in the program, or spin out as a separate program if it grows. Security: sandboxing +
recompile≠run are load-bearing. Depends on the rest of Binary Studio being in place.
