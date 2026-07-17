---
id: CPE-537
title: "Documents — in-app viewer (TOC + markdown + search) + Application → Documents menu"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [needs-prereq]
epic: CPE-534
sprint: SPR-07
estimate: 2-3h
created: 2026-07-16
created: 2026-07-16
---

## Summary
The reader for Application → Documents ([[CPE-534]]): a **DocsView** with a **table-of-contents
sidebar**, the selected doc **rendered as markdown** (reuse the preview markdown renderer), and a
**search** box; opened from a new **Application → Documents** menu item.

## Acceptance Criteria
- [ ] A **Documents** item in the **Application** menu (`MenuBar.svelte`) opens the viewer.
- [ ] The viewer shows a TOC/sidebar of the docs ([[CPE-536]] index), a rendered markdown pane, and a
      search that filters the list.
- [ ] In-doc navigation works (click a doc → it renders); resizable/scrolls; theme-safe; pills reflow.
- [ ] Graceful when a doc is missing / the library is empty.
- [ ] Frontend tests for the viewer's list/search state (pure parts).

## Notes
**needs-prereq:** [[CPE-536]] (the library + index). Reuse the markdown preview renderer + the overlay
panel pattern (BoardView/WorkbenchView).
