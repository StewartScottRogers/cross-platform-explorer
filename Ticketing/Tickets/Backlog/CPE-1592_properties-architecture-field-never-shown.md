---
id: CPE-1592
title: "Properties: the backend computes a binary's architecture (ELF/PE/Mach-O) but never shows it"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-10
---
## Why
Found while writing the new Properties reference page (CPE-1587, epic CPE-1569) and verifying every field
against `PropertiesDialog.svelte` and its backend calls.

`inspect_file` (`crates/server/src/inspect.rs`) returns a `FileInspection` struct that includes an
`architecture: Option<String>` field, populated for ELF/PE/Mach-O executables via `bin_arch::detect_arch`
(e.g. `"x86-64 (64-bit, little-endian)"`). `PropertiesDialog.svelte` already calls `inspectFile` and renders
`inspectionRows` from the response (Encoding / Line endings / File type / the Type-mismatch warning), but
`inspectionRows`'s projection (`PropertiesDialog.svelte`, the `$: inspectionRows = ...` block) never includes
`architecture` — the value comes back from the backend on every properties-open for an executable and is
silently dropped. Grepped the rest of `src/lib/components/*.svelte` for `architecture`/`.arch` — no other UI
surfaces it either.

## Scope
Add an **Architecture** row to `PropertiesDialog.svelte`'s `inspectionRows` (same pattern as the existing
Encoding/Line endings/File type rows — omit the row entirely when `inspection.architecture` is null, same as
every other best-effort field in that dialog). Small, self-contained addition; no backend change needed since
the data is already computed and already in the typed response.

## Acceptance criteria
- Opening Properties on an ELF/PE/Mach-O executable shows an "Architecture" row with the detected string.
- A non-executable file (where `architecture` is null) shows no such row, same as today.
- A `PropertiesDialog.test.ts` case covers it.
- `npm run check` clean; vitest green.

## Notes
Docs-audit find — not user-reported, low severity (nothing is broken, a useful field is just unused). See the
new `src/docs/explorer-properties.md` (CPE-1587) for the full, verified field inventory this ticket rounds out.
