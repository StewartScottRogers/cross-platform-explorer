---
id: CPE-1569
title: "EPIC: Documentation completeness — audit + massively expand in-app docs to cover EVERY feature"
type: Task
status: In Progress
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

## Spike-locked plan (2026-08-10) — see Library `docs-completeness-audit-2026-08-10`
Good news: the pipeline already has **category frontmatter + collapsible TOC + full-text search** (CPE-763) — this is
expansion, not re-architecture. Real **MISSING** pages (Tier 1): Global Undo, Batch Rename, Macros (CPE-739), Link
Forge (CPE-715), Select-by, User-defined commands (CPE-711), a keyboard-shortcut **reference** (36-* only covers the
rebind dialog). **THIN** (Tier 2): Archives, Batch media, Agent Watch (buried, not a Section), Properties, Tags/Smart-
folders/Saved-searches, Workbench, Repos, Agent Deck. Adopt the 10-point **quality rubric** + the "Honest limits"
closing-section house style. Verify against `src/lib/components/`, NOT epic titles (found stale "unbuilt" work-logs for shipped features).

**Child slices (decompose just-in-time; slice 1+2 filed as CPE-1571):**
1+2. **CPE-1571** — IA groundwork (category frontmatter across 37 pages + Index/TOC page) **+** the dialog-doc guard
   test + extended F1 registry for non-Section dialogs. *Foundational; land before content.*
3. Tier-1 batch A: Undo + Batch Rename + Link Forge pages.
4. Tier-1 batch B: Macros + Select-by + User-defined commands pages.
5. Keyboard-shortcut **reference** page (every action + what it does).
6. Split Tags / Smart-folders / Saved-searches out of `03-explorer.md` into own deep pages.
7. Promote Agent Watch to its own page + Section/contextual help.
8. Archives + Batch-media depth pass. 9. Properties reference page. 10. Agent Deck/Workbench/Repos depth.
11. Spotlight hotkey + Terminal shortcuts (small). 12. Screenshot pass via gui-smoke (last, GUI).

Numbering: keep existing `NN-*.md` filenames; NEW pages use category-prefixed slugs (`docs.ts` sorts by frontmatter
`order`, not filename). Every new/expanded page updates `sectionDocs.ts` where it maps to a Section (guard-tested).

## Constraints
- Follow CPE-579: every new page gets its `sectionDocs.ts` entry where it maps to a section; keep
  `sectionDocs.test.ts` green.
- Accuracy over volume — docs must match actual behavior (writers verify against the code/feature, not
  invent). No padding for word-count's sake; depth must be real.
- Reuse the existing `DocsView` + page pipeline; if the corpus grows to hundreds of pages, add navigation/TOC
  + in-docs search as its own slice rather than sprawling flat files.
- Screenshots (if used) via the gui-smoke harness (CPE-1148), not hand-captured.

## Closeout audit 2026-08-29 - KEEP OPEN

All 8 children Done, and the corpus genuinely grew: **37 pages / ~22k words -> 54 pages / 66,156 words**, every `Section` mapped, no stub pages left (thinnest is 210 words). Every Tier-1 MISSING page and every Tier-2 THIN item from the original audit now exists.

A new guard shipped too: `src/docs.coverage.test.ts` reds when a new `*Dialog.svelte` has no doc mention - **and its allowlist-doesn't-rot test is what proves the gaps below are still real today rather than stale entries.**

**Why it stays open: the epic's own instrument enumerates the remainder.**

1. **13 shipped user-facing dialogs have no doc page**, sitting in `KNOWN_GAPS_ALLOWLIST`, which the file itself says CPE-1569's content slices close one at a time: ColorRules, ContentIndexSearch, FileNameSearch, InspectCrypto, KeyboardBindings, PasswordPrompt, RepairLink, ShredConfirm, SignCert, TransferConflict, VaultCreate, WatchRules, Workspaces. (A 14th, PatternSelect, is a documented false positive - ignore it.) The ones a user would hit first: **Workspaces, Color Rules, Watch rules, Content-index search, Find-by-name.**
2. **Spotlight is undocumented** - the component and its hotkey settings ship, and the string "Spotlight" appears in **no** docs page. This was slice 11 and was never filed as a child.
3. **Slice 12, the screenshot pass, never ran** - 0 of 54 pages contain an image.
4. Minor: `05-agent-grid.md` is 210 words for a shipped Section; the depth pass covered Agent *Deck*, not the Grid.

Cost to close: two content tickets (one for the 13 dialogs, one for Spotlight + Agent Grid depth), each retiring allowlist entries. Slice 12 is optional and can be dropped explicitly.
