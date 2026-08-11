---
id: CPE-1604
title: "Docs: split Tags / Smart folders / Saved searches into deep pages, and promote Agent Watch to its own page"
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
Epic CPE-1569 (docs completeness) **slices 6 and 7**, batched because both restructure the same
over-stuffed page. The audit found `03-explorer.md` carrying three substantial features as brief
sub-sections — Tags, Smart folders, Saved searches — each deserving its own deep page, and found **Agent
Watch buried inside another page** despite being one of the app's headline modes with its own design
document (`AGENT-WATCH.md`).

## Scope
1. **Split out three pages** from `03-explorer.md`: Tags, Smart folders, Saved searches. Each gets the full
   treatment the recent Tier-1/Tier-2 pages set as the house standard — what it is, every option and
   action, how it interacts with the rest of the explorer, a worked example, and an honest
   "Limits / notes" closing section. Leave `03-explorer.md` with a short pointer to each rather than a
   duplicate.
2. **Promote Agent Watch to its own page** and give it a `Section` mapping so the toolbar "?" / F1
   contextual help opens it. Read `AGENT-WATCH.md` for the intent, but **document what actually ships** —
   verify every claim against the real components, not the design doc.
3. Category frontmatter + `order` per the CPE-1571 information architecture; cross-link the new pages both
   ways; register anything that maps to a `Section` in `src/lib/sectionDocs.ts` (the guard test
   `sectionDocs.test.ts` fails CI if a `Section` is unmapped or a slug doesn't exist).

## Verify against the code, not the epics
Every claim written against the real components (`src/lib/components/`, the tag/search/watch command
surfaces). Previous docs passes in this epic turned up **five genuine shipped bugs** this way
(CPE-1577, CPE-1584, CPE-1590, CPE-1591, CPE-1592) — that is the most valuable output of this work, not the
prose. **File a bug ticket in `Ticketing/Tickets/Backlog/` for anything you find** (IDs from **CPE-1605**
upward; 1591-1604 are taken) rather than documenting around it.

## Acceptance criteria
- Three new deep pages exist, are registered in `DOCS`, and are reachable in `DocsView`; `03-explorer.md`
  points to them without duplicating them.
- Agent Watch has its own page, and its contextual-help entry opens it.
- `npm run check` green; `sectionDocs.test.ts` and the docs coverage guard green.

## Notes
Model: sonnet. Conflict surface: `src/docs/03-explorer.md`, three new pages, an Agent Watch page,
`src/lib/sectionDocs.ts`. Do **not** touch `src/docs/explorer-archives.md`, `22-file-health.md`,
`30-structured-previews.md`, or any component under `src/lib/components/` — other PRs are in flight there.
