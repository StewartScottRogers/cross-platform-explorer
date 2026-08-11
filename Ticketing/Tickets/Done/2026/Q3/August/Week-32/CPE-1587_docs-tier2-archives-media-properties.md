---
id: CPE-1587
title: "Docs Tier-2: Archives + Batch-media depth pass, and a Properties reference page"
type: Task
status: In Progress
priority: Medium
component: Docs
epic: CPE-1569
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1569 (docs completeness) slices **8 + 9**. The Tier-1 missing pages shipped (CPE-1574/1575/1582);
what remains at the top of the list are the **thin** areas: Archives and Batch media are shallow, and
**Properties** has no reference page at all.

## Scope
1. **Archives depth** — extend the existing archives page: every supported format, browse-inside-archive,
   extract / extract-to / check-safety (the CPE-1578 action-bar actions), safety limits (zip-slip, bombs),
   what is read-only vs. what modifies, and the honest limits.
2. **Batch media depth** — extend the batch-media page: every operation, its options, how selection gates it,
   where results land, failure/partial-success behaviour, and honest limits.
3. **Properties reference page** — a new page documenting the Properties surface exhaustively: every field
   shown, where each value comes from, per-type extras, and how it differs from the preview pane.
4. Follow the epic's **10-point quality rubric** + the "Honest limits" closing-section house style already
   used by the Tier-1 pages. Cross-link related pages.
5. Category frontmatter + `order` per the CPE-1571 IA. If the Properties page maps to a `Section`, add its
   entry to `src/lib/sectionDocs.ts` (guard test `sectionDocs.test.ts` must stay green).

## Verify against the code, not the epics
Write every claim against the **real components** (`src/lib/components/`, `src/lib/preview/*`, the archive +
batch-media command surfaces). The last docs pass found stale "unbuilt" work logs for shipped features — and
surfaced two genuine bugs (CPE-1577, CPE-1584). **File a `CPE-NNN` bug ticket in `Backlog/` for anything the
docs work uncovers** rather than documenting around it silently.

## Acceptance criteria
- Archives + Batch-media pages are genuinely deep (every option/action covered, examples where they help).
- A Properties reference page exists, is registered in `DOCS`, and is reachable in `DocsView`.
- `npm run check` green; `sectionDocs.test.ts` green; docs render correctly in the Documents library.

## Notes
Model: sonnet. Conflict surface: `src/docs/*.md` (archives, batch-media, new properties page) +
possibly `src/lib/sectionDocs.ts`. Do **not** edit `36-keyboard-shortcuts.md`,
`input-keyboard-reference.md`, `organizing-user-commands.md` (another worker owns those), and do not touch
`provider.ts` / `PreviewPane.svelte` / `App.svelte`.
