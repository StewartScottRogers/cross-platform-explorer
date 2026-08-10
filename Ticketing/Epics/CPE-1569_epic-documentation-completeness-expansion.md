---
id: CPE-1569
title: "EPIC: Documentation completeness — audit + massively expand in-app docs to cover EVERY feature"
type: Task
status: Proposed
priority: High
component: Frontend
tags: [epic]
created: 2026-08-10
closed:
---

> **Filed 2026-08-10 (user request).** Umbrella epic — decomposed just-in-time from the docs-audit spike
> (dispatched 2026-08-10). Dormant brief until activated.

## Why (user's words)
"Review the completeness of our documentation feature and extend it for **all** the features we have
implemented. I would expect the document system should be **~200× the size it is now**." The in-app Documents
library (CPE-534 / CPE-579) currently ships **37 pages / ~22k words / 48 section mappings**, but the app has
25+ completed epics and dozens of features, settings, shortcuts, and workflows — coverage is broad-but-shallow.
The user wants **exhaustive, deep** documentation: every feature, sub-feature, setting, action, keyboard
shortcut, and end-to-end workflow, with examples. The 200× figure is the north-star ambition (depth +
completeness), not a literal word-count acceptance gate.

## The existing system (extend, don't replace)
- `src/docs/*.md` — the 37 page corpus (the source of truth for in-app docs).
- `src/lib/sectionDocs.ts` — the `section → doc slug` registry (48 entries); guarded by
  `src/lib/sectionDocs.test.ts` (every `Section` must map; every slug must exist in `DOCS`).
- `DocsView` renders pages; contextual help (toolbar "?" / F1) opens the current section's page.
- CPE-579 rule: every user-facing section ships/updates its page + registry entry (guard-tested).

## Goal
The in-app docs become a **comprehensive reference + guide** covering every shipped feature end to end:
per-feature reference (what it is, every option/setting, every action + its keyboard shortcut), how-to guides
for real workflows, cross-links between related features, a searchable structure, and examples/screenshots
where they help. Depth target: each major feature/epic gets thorough multi-section coverage rather than a
single thin page.

## Decomposition (pending the audit spike — the spike produces the concrete plan)
The spike delivers: (1) a coverage matrix of every shipped feature/epic vs its current doc page(s) — thorough
/ thin / missing; (2) a proposed information architecture (reference vs how-to vs tutorial; page taxonomy;
naming/numbering scheme that scales to hundreds of pages); (3) a docs-quality rubric (what "complete" means
per feature); (4) a prioritized slice list — batches of pages grouped by feature area, each an independently
buildable child ticket; (5) any tooling gaps (search across docs, an index/TOC page, screenshot capture via
the gui-smoke harness, a lint/guard that flags a shipped feature with no deep page). Child tickets filed once
the spike returns and the epic is activated.

## Constraints
- Follow CPE-579: every new page gets its `sectionDocs.ts` entry where it maps to a section; keep
  `sectionDocs.test.ts` green.
- Accuracy over volume — docs must match actual behavior (writers verify against the code/feature, not
  invent). No padding for word-count's sake; depth must be real.
- Reuse the existing `DocsView` + page pipeline; if the corpus grows to hundreds of pages, add navigation/TOC
  + in-docs search as its own slice rather than sprawling flat files.
- Screenshots (if used) via the gui-smoke harness (CPE-1148), not hand-captured.
