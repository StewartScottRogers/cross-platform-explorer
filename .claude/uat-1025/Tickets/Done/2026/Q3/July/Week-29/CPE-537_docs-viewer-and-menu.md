---
id: CPE-537
title: "Documents — in-app viewer (TOC + markdown + search) + Application → Documents menu"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [needs-prereq]
epic: CPE-534
sprint: SPR-07
closed: 2026-07-16
estimate: 2-3h
created: 2026-07-16
created: 2026-07-16
---

## Summary
The reader for Application → Documents ([[CPE-534]]): a **DocsView** with a **table-of-contents
sidebar**, the selected doc **rendered as markdown** (reuse the preview markdown renderer), and a
**search** box; opened from a new **Application → Documents** menu item.

## Acceptance Criteria
- [x] A **Documents** item in the **Application** menu (`MenuBar.svelte`) opens the viewer.
- [x] The viewer shows a TOC/sidebar of the docs ([[CPE-536]] index), a rendered markdown pane, and a
      search that filters the list.
- [x] In-doc navigation works (click a doc → it renders); resizable/scrolls; theme-safe; pills reflow.
- [x] Graceful when a doc is missing / the library is empty.
- [x] Frontend tests for the viewer's list/search state (pure parts).

## Notes
**needs-prereq:** [[CPE-536]] (the library + index). Reuse the markdown preview renderer + the overlay
panel pattern (BoardView/WorkbenchView).

## Resolution
Added the in-app documents viewer + the Application menu entry.

- **`DocsView.svelte`** — a bordered overlay with a **TOC sidebar** (the CPE-536 index) + a **search
  box** (`searchDocs`), the selected doc **rendered as sanitized markdown** via the existing
  `renderMarkdown` (marked + DOMPurify), with scoped doc typography. Selection stays valid as the filter
  narrows; empty/no-match handled.
- **Application → Documents** — the Application menu's item now opens the in-app viewer (was: external
  repo). New i18n key `mi.documents` added to all four catalogs (en/es/de/fr); `onMenuSelect("documents")`
  → `showDocs`.
- Per the **standing docs rule**, extended `01-overview.md` to point at Application → Documents.

`npm run check` clean; 555 frontend tests pass (i18n balance held). Second ticket of SPR-07 — completes
the Application → Documents epic CPE-534.

## Work Log
2026-07-16 — Picked up (SPR-07; prereq CPE-536). Built DocsView (TOC + markdown + search), repointed the Application menu item to the in-app viewer (+ mi.documents in 4 locales), extended the overview doc. npm check clean; 555 tests. All ACs met.
