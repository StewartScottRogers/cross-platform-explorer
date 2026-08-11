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

## Work Log

**2026-08-11 — frontend wiring done, PR opened**

Branch `cpe-1615-dotnet-metadata-tab`. Zero backend changes — `dotnet_metadata.rs`, `model.rs`, and the
`dotnet_metadata` Tauri command were untouched, exactly as scoped.

- `src/lib/preview/binaryInspector.ts` — removed `managedDotNetConfidence`/`ManagedConfidence`/
  `EMPTY_TABLES_NORMAL_EXTS`/`emptyImportExportIsNormalFor` entirely (no heuristic left behind). Added
  three small pure formatters for the new tab: `decodeAssemblyFlags` (known ECMA-335 `AssemblyFlags` bits
  → pill labels), `cultureLabel` (null/empty → "neutral"), `hexOrDash` (null/empty hex blob → em dash).
- `src/lib/components/BinaryPreview.svelte` — `managed` now reads `info.is_managed` directly (no more
  hedged "confirmed"/"possible" wording — the backend flag is a real CLR-header read, so the caveat states
  plainly that a file is/isn't managed). Added a ".NET metadata" tab, shown only when `is_managed`,
  reusing the same `.tab`/`.tab.active` classes as every other tab (docs/design/TABS.md — no new tab
  style). Lazy-fetches `commands.dotnetMetadata(path)` the first time the tab is opened, mirroring the
  Disassembly tab's request-id-guarded lazy-fetch pattern (stale in-flight requests are dropped, state
  resets on file change). Distinguishes three outcomes explicitly: `loaded` + `dotnetMeta === null`
  (metadata root absent/unparseable — real, valid "nothing here" response) vs `error` (the command threw —
  unreadable/malformed) vs the populated case — the null and error states use separate testids
  (`binary-dotnet-null` vs `binary-dotnet-error`) and never render as a clean/empty table. Assembly
  identity, referenced-assembly, type, and method tables all go through the existing `capRows`/
  `BINARY_TABLE_ROW_CAP` capping with an honest "Showing the first N of M" note, same as
  Sections/Imports/Exports/Symbols. Assembly-flags pills use the reflowing `flex-wrap` container +
  `white-space:nowrap; flex:0 0 auto` pills per the project's tick-tack convention. All colours are
  existing semantic tokens already used elsewhere in this file (`--text`, `--text-dim`, `--surface-alt`,
  `--border`) — no new tokens, so no WCAG-guard-test changes needed.
- `src/lib/components/PreviewPane.svelte` — dropped the now-dead `extension={entry.extension}` prop passed
  to `<BinaryPreview>` (the extension-keyed heuristic that consumed it is gone).
- `src/lib/preview/binaryInspector.test.ts` — replaced the retired heuristic's tests with unit tests for
  `decodeAssemblyFlags`/`cultureLabel`/`hexOrDash`.
- `src/lib/components/BinaryPreview.test.ts` — added `dotnetMetadata` to the mocked `commands` object;
  local `BinaryInfo` mock type gained `is_managed`; rewrote the managed-.NET gating tests to assert on
  `is_managed` directly (dropped the old confirmed/possible/`.efi`/`.sys`-carve-out cases, since that
  branching no longer exists); added a new "BinaryPreview — .NET metadata tab" describe block covering:
  tab hidden for a native binary, tab shown + not fetched until opened for a managed one, populated
  render (identity/refs/types/methods + flag pill), the null-result state, the error state, the
  no-assembly-manifest (module) state, empty-list states, a capped-table case, and reset-on-file-change.

**Verification (all run synchronously in the worktree, not backgrounded):**
- `npm run check` → `svelte-check found 0 errors and 0 warnings`.
- `npx vitest run` (full suite) → `Test Files 272 passed (272)`, `Tests 3312 passed (3312)`.
- `npx vitest run src/lib/preview/binaryInspector.test.ts src/lib/components/BinaryPreview.test.ts` →
  `Test Files 2 passed (2)`, `Tests 45 passed (45)`.

No new dependencies. No Rust files touched (`git status --porcelain` shows only the 5 frontend files above
plus this ticket's move to `Doing/`). Per jsdom's known blind spot for real layout/clipping, these tests
verify data/state transitions only — the tab's on-screen tab-strip/pill-reflow look still needs the
screenshot/Visual-Critic pass, not a jsdom claim.
