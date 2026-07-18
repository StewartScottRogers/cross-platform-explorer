---
id: CPE-686
title: Document drag-and-drop + collapsible sidebar in the in-app docs
type: docs
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
---

## Summary
Dayshift docs-gap fill (per [[maintain-in-app-docs-library]]). The in-app Explorer doc (`src/docs/03-explorer.md`)
had **zero** mention of drag-and-drop despite this session shipping a whole unified DnD system (CPE-661
children 668–673), and didn't mention the new collapsible sidebar sections (CPE-675). Added both.

## Acceptance Criteria
- [x] A "Drag and drop" bullet documents internal move/copy (OS convention + Ctrl/Shift override + count
      badge) and OS drop-in (copy to folder under cursor), noting the transfer-panel + progress.
- [x] The Sidebar bullet notes collapsible, persisted sections.
- [x] Docs guard (`sectionDocs.test.ts`) + i18n gate green; no new Section, so no registry change needed.

## Resolution
Edited `src/docs/03-explorer.md`: new **Drag and drop** bullet in the Files section + a collapsible-sections
note on the **Sidebar** bullet. Markdown-only, no code/section change. Guard + i18n tests green.
