---
id: CPE-1537
title: "Theme foundation: Appearance docs page + sectionDocs registry entry (CPE-579)"
type: Feature
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-1492
created: 2026-08-09
---
## Context
CPE-579 ("in-app docs are self-maintaining") requires every user-facing section to ship a doc page in
`src/docs/*.md` and register it in `src/lib/sectionDocs.ts`; the guard test
`src/lib/sectionDocs.test.ts` fails CI if a section is unmapped or a mapped slug is missing. CPE-1492
adds a new user-facing surface (the Appearance control in Settings, CPE-1536) and this closes that
documentation gap the same way `[[maintain-in-app-docs-library]]` requires for every other feature —
compare `network`/`drop-stack`/`vaults`, which are "not a sidebar view you switch into" (Appearance is a
Settings row, same shape) but still earn a doc page + registry entry per that convention.

## Scope
- New `src/docs/35-appearance.md` (`order: 35`, pick a `category`/`categoryOrder` consistent with
  neighboring settings-adjacent docs — check an existing settings-related doc like
  `19-terminal.md`/`20-vaults.md` for the convention to match) explaining: what the Appearance setting in
  Settings does today (System/Light, both currently render the same light palette), and that it's the
  foundation for OS-following dark mode landing in a later epic (CPE-1493) — write it as "this is where
  it lives," not a feature-complete dark-mode announcement.
- Add `"appearance"` to the `Section` union type and `SECTION_DOC` map in `src/lib/sectionDocs.ts`
  (`:11-33` and `:36-120`), pointing at the new `35-appearance` slug — follow the exact comment style
  used for the other non-sidebar entries (`vaults`, `terminal`, `drop-stack`) explaining it's a Settings
  row, not a switched-into view.
- No other file changes — this ticket is docs + registry only, no behavior.

## How
- Copy the frontmatter shape and prose length of `src/docs/34-drop-stack.md` exactly.
- Add the registry entry as the last item in `SECTION_DOC` (matching the file's existing append-only
  convention — new entries added at the end with their own explanatory comment, most recently
  `drop-stack`).

## Verify
`npm run check`; `npx vitest run src/lib/sectionDocs.test.ts` — the existing exhaustiveness guard test
passes with the new `appearance` section mapped and the `35-appearance` slug present in `DOCS`. Fully
headless — no GUI verification required (this ticket is docs content + a registry map, no runtime
behavior to exercise).

## Notes
**Conflict surface:** new `src/docs/35-appearance.md` and an additive append to
`src/lib/sectionDocs.ts` (new union member + new map entry at the end — no edits to existing entries).
No `src/App.svelte` edits. **Dispatch order: after CPE-1536** (the doc should describe the Appearance
row that ticket actually ships, not a speculative one) — otherwise touches none of the same files as any
sibling ticket, so it's safe to write in parallel and just needs CPE-1536 to have landed before merge/
review for accuracy.
