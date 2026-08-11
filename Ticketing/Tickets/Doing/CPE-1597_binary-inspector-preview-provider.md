---
id: CPE-1597
title: "Binary Inspector preview provider — tabbed Overview / Sections / Imports / Exports / Symbols / Disasm"
type: Feature
status: In Progress
priority: Medium
component: Frontend
epic: CPE-1562
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1562 (Binary Inspector) **slice 4** — the payoff slice. The whole backend now exists and, as of
CPE-1585 (PR #800), is callable from the frontend via typed bindings: `binaryInfo(path)` returns format,
architecture, sections, imports, exports and symbols; `binaryDisasm(path)` returns a capped x86/x64
disassembly listing. Nothing in the UI consumes any of it — selecting an `.exe`/`.dll`/`.so`/`.dylib` still
shows the generic fallback.

## Goal
A **Binary Inspector** preview provider on the existing preview seam that renders a tabbed read-only view of
the selected executable.

## Scope
- New provider in `src/lib/preview/provider.ts` for executable/library types (PE `.exe`/`.dll`, ELF `.so` and
  extensionless ELF where detection already tells us, Mach-O `.dylib`). Reuse the existing registry and the
  CPE-1570 declarative `actions?: PreviewAction[]` action bar — **do not fork either**.
- A tabbed component: **Overview** (format, architecture, bitness, entry point, sizes), **Sections**,
  **Imports**, **Exports**, **Symbols**, **Disassembly**. Tables must be virtualized/capped — a binary with
  thousands of imports must not stall the pane (STREAMING.md + PURPOSE.md's fast/small/predictable rule).
- Call the backend through `src/lib/invoke.ts` (never `@tauri-apps/api/core`) using the generated typed
  bindings. **Only fetch a tab's data when that tab is opened** — do not call `binaryDisasm` on selection.
  This matters: `binaryDisasm` currently re-runs the full parse (a known, measured ~1.9× cost), so an eager
  fetch would double the work for a user who never opens the Disasm tab.
- **Managed .NET honesty**: the disassembler decodes CIL as if it were x86 for a managed assembly, producing
  meaningless output (confirmed by UAT on CPE-1585 against `mscorlib.dll`). Do not present that as real
  disassembly. Detect a managed image if the backend exposes it (CPE-1596 is adding that flag in parallel) —
  otherwise detect it frontend-side or, at minimum, caveat the Disasm tab clearly. Never show nonsense as if
  it were fact.
- Empty/error states: a file that isn't a recognised binary, an oversized file (the backend rejects above its
  preview cap with a clear message), and a permission error must all read as calm explanations, not stack
  traces.
- Theme tokens only (no hard-coded colours — a real dark theme ships); any chips/pills follow the reflow rule.
- **Docs per CPE-579**: give this its own page `src/docs/binary-inspector.md` (it is substantial enough to
  warrant one) rather than a section inside `30-structured-previews.md` — another worker owns that file right
  now. If it maps to a `Section`, add the `src/lib/sectionDocs.ts` entry; otherwise leave that file alone.
- Tests: provider selection by type, per-tab lazy fetching (assert `binaryDisasm` is NOT called until the
  Disasm tab opens), table capping, and the error/empty states.

## Acceptance criteria
- Selecting a real executable shows the tabbed inspector populated with real data.
- Opening the Disasm tab (and only then) fetches the disassembly; the listing is capped.
- A managed .NET assembly does not present bogus disassembly as genuine.
- `npm run check` + frontend tests green.

## Notes
Model: sonnet. Conflict surface: `src/lib/preview/provider.ts`, a new component under
`src/lib/components/`, `src/docs/binary-inspector.md`, possibly `src/lib/sectionDocs.ts`, plus i18n keys.
Do NOT touch `src/lib/preview/font.ts`, `FontPreview.svelte`, `src/docs/30-structured-previews.md`
(CPE-1593 owns those), `src-tauri/src/lib.rs`, or `src/lib/bindings.gen.ts` (CPE-1596 owns those).
