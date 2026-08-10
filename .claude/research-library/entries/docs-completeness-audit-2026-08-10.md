---
title: "How complete are the in-app docs, and how to expand them to cover every feature (CPE-1569)?"
slug: docs-completeness-audit-2026-08-10
date: 2026-08-10
status: current
tags: [cpe-1569, docs, sectionDocs, docsview, information-architecture, coverage-gap, guard-test, pm-reference]
---

## Pipeline is more scalable than assumed (extend, don't re-architect)
`src/lib/docs.ts` already supports **frontmatter `category`/`categoryOrder`**; `DocsView.svelte` already **groups
into collapsible TOC categories + has working title+full-text search** (CPE-763). It's a taxonomy-ready shell, just
under-populated (4 categories today). `sectionDocs.ts` + guard (CPE-579/597) enforce "every Section→doc, every slug
exists" — BUT only covers features reachable via contextual F1 from a `Section`. Dialog/key features (Macros, Link
Forge, Batch Rename, Select-by, Undo) aren't Sections → no guard → shipped undocumented. House style worth codifying:
"Honest limits"/"What this is (and isn't)" closing sections (in vaults/copilot/cert/split-join pages).

## Coverage: 37 pages, ~22k words. Verdicts
**MISSING (shipped, zero docs) — Tier 1, highest value:** Global **Undo** (Ctrl+Z), **Batch Rename** (4 modes),
**Macros/scriptable actions** (CPE-739), **Link Forge** New Link (CPE-715), **Select by…** dialog, **User-defined
commands** (CPE-711), a real **keyboard-shortcuts REFERENCE** page (36-* only documents the rebind dialog, not what
keys do). NOTE: audit found CPE-715 & CPE-739 epic work-logs stale ("unbuilt") vs actual shipped UI — always verify against `src/lib/components/`.
**THIN (shallow) — Tier 2:** Archives (no format/password matrix), Batch media (8 ops' settings), Agent Watch (buried
at tail of 03-explorer, not a Section), Properties dialog (scattered), Tags/Smart-folders/Saved-searches (1 para each
in 03-explorer), Workbench, Repositories, Agent Deck. **THOROUGH:** most others (12-search is the best page).

## Proposed IA (grow taxonomy, keep pipeline)
~12 categories: Getting started · Explorer(core) · Organizing&Tagging · Search&Discovery · Safety&Recovery ·
Previews&Media · Power Tools · Network&Remote · Appearance&Input · Agent Workspace · Development · Reference(+Index page).
**Numbering:** current flat `NN-slug.md` breaks past 99 + doesn't encode category; `docs.ts` sorts by frontmatter
`order` NOT filename → switch NEW pages to category-prefixed slugs (`explorer-batch-rename.md`), keep existing 37
filenames (zero churn). Split Reference/How-to once an area hits 3+ pages. Formalize cross-links (2-3 per page intro).
Add an Index/TOC page as first entry. Existing search holds past 100 pages — defer search work.

## Quality rubric ("complete" = all 10)
1 what-it-is · 2 when-to-use (vs adjacent) · 3 how-to-open (menu+palette+shortcut) · 4 every option enumerated (+default
state) · 5 every action+shortcut (table if 3+) · 6 ≥1 worked example · 7 cross-links · 8 REQUIRED "limits/honesty"
closing section · 9 safety framing for destructive actions · 10 accuracy verified against the component, not the epic title.

## Tooling gaps → slices
1. **Guard test: shipped-feature-without-a-doc detector** (scan user-facing dialog components, assert each referenced in
   src/docs) — would've caught Macros/Link-Forge/Batch-Rename/Select-by. 2. Extend `sectionDocs`-style registry to
   non-Section dialogs (dialog/action id→slug) for F1. 3. Screenshot pass via gui-smoke (CPE-1148) for visual pages —
   after text lands. 4. Skip raw word-count lint (epic's no-padding rule); prefer rubric-section presence check.

## Slice plan → CPE-1569 children (Foreman assigns IDs)
1 IA groundwork (category frontmatter across 37 + Index page). 2 dialog-doc guard test + extended help registry (land
before/with content). 3 Tier-1 batch A: Undo + Batch Rename + Link Forge. 4 Tier-1 batch B: Macros + Select-by +
User-commands. 5 keyboard-shortcuts reference page. 6 split Tags/Smart-folders/Saved-searches out of 03-explorer. 7
promote Agent Watch to own page+Section. 8 Archives+Batch-media depth. 9 Properties reference page. 10 Agent
Deck/Workbench/Repos depth. 11 Spotlight hotkey + Terminal shortcuts (small). 12 screenshot pass (GUI, last).
All headless-verifiable (markdown + guard) except slice 12.
