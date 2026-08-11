---
id: CPE-1596
title: ".NET/CLR metadata reader — hand-rolled ECMA-335 tables (assembly, refs, typedef, methoddef)"
type: Feature
status: In Progress
priority: Medium
component: Backend
epic: CPE-1562
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1562 (Binary Inspector) **slice 3**. Slices 1 (CPE-1572 `BinaryInfo` DTO), 2 (CPE-1581 x86/x64
disassembly) and the command wiring (CPE-1585) have merged. Managed .NET binaries are the remaining blind
spot: they parse as PE, but their real content is CLR metadata, not native code. UAT on CPE-1585 confirmed
the concrete symptom — running the x86 disassembler over `mscorlib.dll` produces 2,048 "instructions" that
are the decoder chewing on CIL bytecode, i.e. meaningless output presented as if it were real.

## Goal
A pure-Rust, in-process reader for the CLR metadata of a managed PE, surfaced as a structured DTO alongside
`BinaryInfo`, so a managed assembly is legible before the full decompile epic (CPE-1563) lands.

## Scope
- **Hand-roll the ECMA-335 reader.** `dotnetdll` is GPL and therefore NOT usable in-process — this is a hard
  constraint, do not add it. `goblin` (already a dependency) locates the PE and the CLI header in data
  directory #14; from there parse the metadata root, the stream headers, and the `#~` compressed table stream.
- Extract, at minimum: the **assembly manifest** (name, version, culture, public-key token, flags), the
  **referenced assemblies** (`AssemblyRef`), and the **type/method tables** (`TypeDef`, `MethodDef`) with
  their names resolved through the `#Strings` heap.
- Structured DTO in `cpe-server::model` next to the existing binary DTOs, following their shape and their
  `MAX_BINARY_LIST_ENTRIES`-style caps. Every list must be **capped** and every parse **skip-on-error** —
  never fail the whole inspection because one table is malformed.
- **Detect managed images** and expose that fact on `BinaryInfo` so callers can tell a managed PE from a
  native one. This is what lets the frontend stop presenting nonsense disassembly for a managed image.
- Thin async `#[tauri::command]` dispatcher in `src-tauri/src/lib.rs` following the CPE-1585 pattern
  (`spawn_blocking` + the existing size guard), registered in `generate_handler![]` **and** the
  `specta_commands!` list; regenerate `src/lib/bindings.gen.ts`.

## Parser safety — this is the top fuzz priority in the epic
A metadata reader walks file-supplied offsets, counts and heap indices. It must never panic, hang, or read
out of bounds on a truncated, corrupt or hostile assembly. Watch specifically for the two parser bug patterns
this repo has been bitten by: **slicing a string at a byte offset** and **a dead truncation notice after a
capped read**. Add fixture tests for: a native (unmanaged) PE, a truncated managed PE, a bogus CLI header
RVA, absurd table row counts, and heap indices past the end. Fuzz the entry point.

## Acceptance criteria
- A real managed assembly (e.g. `C:\Windows\Microsoft.NET\Framework64\v4.0.30319\mscorlib.dll`) yields its
  assembly identity, its `AssemblyRef` list, and a capped `TypeDef`/`MethodDef` listing with real names.
- A native PE, a truncated file, a zero-byte file, and a renamed text file all return a clean error or an
  empty-but-valid result — never a panic or a hang.
- `cargo clippy --all-targets -- -D warnings` clean in both feature modes; `cargo test` green.
- `npm run check` green; `bindings.gen.ts` regenerated + committed (drift guard green).
- Both `Cargo.lock`s regenerated if any dependency changed.

## Notes
Model: sonnet (escalate if the table parsing turns gnarly). Conflict surface: `crates/server/src/` (new
module + `model.rs`), `src-tauri/src/lib.rs`, `src/lib/bindings.gen.ts`. Do NOT touch `src/lib/preview/*`,
`PreviewPane.svelte`, `App.svelte`, or `src/docs/*` — other workers are there.
Library entry: `binary-studio-engines-delivery-2026-08-10`.
