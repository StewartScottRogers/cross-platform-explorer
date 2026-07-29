---
id: CPE-927
title: Agent Board Docs button dead + search-box Docs button missing
type: bug
component: Frontend
priority: high
tags: ready
created: 2026-07-23
status: Done
---

## Summary
Two broken/missing docs affordances:
1. **Agent Board "Docs" button does nothing.** In the standalone board window, `AgentBoardApp` handled
   BoardView's `help` event with a no-op (`/* CPE-845 */`), so the button was dead. Now it opens the
   Agent Board doc page (`06-agent-board`) in an in-window `DocsView` overlay.
2. **The search BOX had no Docs button.** CPE-921 added one to the search *dialogs*, but the actual
   "Search Box" is the toolbar search input (magnifying glass). Added a small book/Docs button inside it
   that opens the search-options page (`12-search`).

## Acceptance Criteria
- [x] Agent Board Docs button opens the docs (Agent Board page) in its window.
- [x] The toolbar search box shows a Docs button that opens the search-options page.
- [x] `npm run check` passes.

## Work Log
- 2026-07-23 — Filed + fixed both.

- 2026-07-23 — Fixed both, browser-verified: AgentBoardApp opens DocsView (agent-board page) on help; NavToolbar search box has a book Docs button dispatching searchDocs → App openDocsSlug("12-search"). Screenshots confirm both open the right doc page. check clean.
