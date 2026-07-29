---
id: CPE-564
title: "Agent Board — copy a card's ticket id to the clipboard"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
epic: CPE-503
estimate: 30m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
On a ticketing board you constantly reference a ticket id (in commits, branches, chat). Add a small
copy-id button to each card so it's one click, instead of retyping "CPE-541".

## Acceptance Criteria
- [x] Each card (both the Kanban lanes and the Epics view) has an unobtrusive copy-id button that copies
      the ticket id to the clipboard, with a brief ✓ confirmation.
- [x] The button doesn't interfere with dragging the card (`stopPropagation` on click/mousedown).
- [x] `npm run check` clean; a component test covers the copy action.

## Resolution
`BoardView` gained `copyId(id)` (`navigator.clipboard.writeText`, defensive try/catch, brief `copiedId` ✓
state cleared after ~1.1s) and a `.card-copy` button in both the lane card `.card-top` and the epic
`.ecard-top`. It's revealed on card hover (unobtrusive), and `on:click|stopPropagation` +
`on:mousedown|stopPropagation` keep it clear of the card drag. Component test drives a card → clicks
**Copy CPE-42** → asserts `clipboard.writeText("CPE-42")`. Full suite **600 pass / 62 files**;
`npm run check` 0/0. Board UI (non-localized); no i18n change.

## Notes
Ran the full suite before landing (per the CPE-563 lesson).
