---
id: CPE-1615
title: "Binary Inspector: real .NET metadata tab, using the backend flag/reader that already ship"
type: Feature
status: Backlog
priority: High
component: Frontend
epic: CPE-1562
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Epic CPE-1562 (Binary Inspector) named this as its recommended next slice, and the backend side is
**already fully built and merged**: `crates/server/src/dotnet_metadata.rs::read` (hand-rolled ECMA-335
reader, CPE-1596), the `dotnet_metadata` Tauri command (`src-tauri/src/lib.rs:965`), and a real
`is_managed: bool` field on `BinaryInfo` (`crates/server/src/model.rs:224`, `bindings.gen.ts:3648`) all
shipped and merged (`Ticketing/Tickets/Done/2026/Q3/August/Week-32/CPE-1596_*.md`). But
`src/lib/preview/binaryInspector.ts` still carries a frontend-only heuristic
(`managedDotNetConfidence`) with its own TODO pointing at exactly this gap:

> `TODO(CPE-1596): that ticket is adding a real is_managed flag to BinaryInfo... once it lands, prefer
> info.is_managed directly and retire this whole heuristic`

And `src/lib/components/BinaryPreview.svelte` has six tabs (Overview / Sections / Imports / Exports /
Symbols / Disasm) but no ".NET metadata" tab at all — a managed assembly's real content (assembly
identity, referenced assemblies, types, methods) is invisible even though the reader that produces it has
shipped. This is the single remaining piece of epic CPE-1562's "recommended first build."

## Goal
1. Swap the frontend's guessing heuristic for the real backend flag.
2. Surface the CLR metadata the backend already parses in a new tab.

## Scope
**Conflict surface:** `src/lib/components/BinaryPreview.svelte`, `src/lib/components/BinaryPreview.test.ts`,
`src/lib/preview/binaryInspector.ts`, `src/lib/preview/binaryInspector.test.ts`,
`src/docs/binary-inspector.md`. No backend changes (the command/DTO already exist) and no overlap with any
other ticket on this bench — safe to build fully in parallel with everything else.

- Replace `managedDotNetConfidence`'s "possible"/"confirmed" guessing with `info.is_managed` directly for
  gating the Disasm-tab caveat (the CIL-vs-x86 warning). Retire `EMPTY_TABLES_NORMAL_EXTS` and the
  imports/exports-based guess entirely — `is_managed` is a real CLR-header read, not a heuristic, so the
  whole "possible" tier and its `.efi`/`.sys` carve-out become dead code once the real flag is wired
  in. Keep the wording honest (no caveats needed once the flag is authoritative — either it's managed or
  it isn't).
- Add a **".NET metadata" tab**, shown only when `info.is_managed` is true, that lazily calls
  `commands.dotnetMetadata(path)` (mirror the existing lazy Disasm-tab fetch pattern: fetch once on first
  visit, guard against stale in-flight requests by request id, reset on file change) and renders the
  `DotnetMetadata` result:
  - Assembly identity (`assembly`, nullable): name, version, culture, public key (hex), flags.
  - Referenced assemblies (`assembly_refs`): name/version/culture/public-key-token table.
  - Types (`types`): name (+ namespace) table, capped/labelled the same way Sections/Imports/etc. already
    are (`capRows`/`BINARY_TABLE_ROW_CAP`, "Showing the first N of M" note) — `types`/`methods`/
    `assembly_refs` can legitimately number in the thousands for a large assembly.
  - Methods (`methods`): name table, same capping.
  - Handle the `null` result (metadata root absent/unparseable) and the loading/error states the same way
    the Disasm tab does — never a blank pane with no explanation.
- Update `src/docs/binary-inspector.md` to document the new tab (CPE-579: every user-facing section keeps
  its doc page current).

## Explicitly NOT in scope
- No changes to `dotnet_metadata.rs`, `model.rs`, or the Tauri command — that surface is done.
- No decompilation of method bodies — this ticket surfaces names/identity only, matching what
  `dotnet_metadata::read` already returns (epic CPE-1563 owns actual decompilation).

## Acceptance criteria
- Opening a real managed assembly (e.g.
  `C:\Windows\Microsoft.NET\Framework64\v4.0.30319\mscorlib.dll`) shows a ".NET metadata" tab with its
  real assembly identity, referenced-assembly list, and capped type/method tables.
- Opening a native PE/ELF/Mach-O shows no ".NET metadata" tab, and the Disasm tab's managed-caveat is
  gated on `info.is_managed`, not the old heuristic.
- `managedDotNetConfidence`/`EMPTY_TABLES_NORMAL_EXTS`/`emptyImportExportIsNormalFor` are removed (or
  reduced to the minimum still needed) once `is_managed` does the gating — no dead heuristic left behind.
- `npm run check` and the Vitest suite (`binaryInspector.test.ts`, `BinaryPreview.test.ts`) green.
- Large tables stream/cap per `docs/design/STREAMING.md`'s spirit — never stall the pane on a big assembly.

## Notes
Model: sonnet. Library entry: `binary-studio-engines-delivery-2026-08-10`.
