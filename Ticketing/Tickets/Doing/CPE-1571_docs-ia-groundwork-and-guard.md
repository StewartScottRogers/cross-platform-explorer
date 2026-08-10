---
id: CPE-1571
title: "Docs IA groundwork: category frontmatter + Index page + shipped-feature-without-a-doc guard test"
type: Task
status: Doing
priority: High
component: Frontend
epic: CPE-1569
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1569 (documentation completeness), foundational slices 1+2 — must land before the content batches so new
pages slot into a real taxonomy and can't silently go undocumented again. The docs pipeline already supports
categories + TOC + search (`src/lib/docs.ts`, `src/lib/components/DocsView.svelte`, CPE-763) — this populates it.

## Scope
1. **IA / categories:** add `category` + `categoryOrder` frontmatter to the existing 37 `src/docs/*.md` pages,
   grouping them into the ~12 categories from the audit (Getting started · Explorer · Organizing & Tagging · Search &
   Discovery · Safety & Recovery · Previews & Media · Power Tools · Network & Remote · Appearance & Input · Agent
   Workspace · Development · Reference). **Content unchanged** — pure frontmatter reorganization. Verify `DocsView`
   renders the new grouping.
2. **Index/TOC page:** add a new first-entry Index page listing every feature category → its pages (markdown links;
   `renderMarkdown` already handles `<a>`).
3. **Guard test — shipped-feature-without-a-doc detector:** a vitest that scans user-facing dialog components in
   `src/lib/components/*.svelte` (by a naming convention like `*Dialog.svelte` and/or an explicit maintained registry
   list) and asserts each is referenced by name/keyword somewhere in `src/docs/*.md`. Seed the registry with the known
   gaps (Macros, Link Forge, Batch Rename, Select-by, User-commands) marked as **expected-missing/allowlisted** for now
   so the test passes today but will fail if a NEW undocumented dialog ships — and shrink the allowlist as CPE-1569
   content slices land.
4. **Extended F1 registry (optional within this ticket if cheap):** a lightweight "dialog/action id → doc slug" map so
   contextual help can target non-Section dialogs; wire it only if it doesn't balloon scope — else split to a follow-up.

## Acceptance criteria
- `DocsView` shows the pages grouped by the new categories in `categoryOrder`; search still works.
- Index page renders and links resolve.
- The new guard test passes on the current tree and would fail if a non-allowlisted user-facing dialog had no doc mention.
- `sectionDocs.test.ts` stays green; `npm run check` clean; vitest green.
- No page CONTENT rewritten in this ticket (frontmatter + new Index page + test only).

## Notes
Keep existing `NN-*.md` filenames (docs.ts sorts by frontmatter `order`, not filename — zero-churn). Frontend/docs-only;
disjoint from the preview-pane and trash work. See Library `docs-completeness-audit-2026-08-10`. Model: sonnet.
