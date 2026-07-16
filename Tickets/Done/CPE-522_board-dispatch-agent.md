---
id: CPE-522
title: "Agent Board — Dispatch a card to a scoped agent session"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [needs-prereq]
epic: CPE-503
sprint: SPR-04
closed: 2026-07-16
estimate: 3-4h
created: 2026-07-16
---

## Summary
Wave 2 of [[CPE-503]]: an explicit **Dispatch** action on a card (activation decision — not auto-launch
on drag) that launches a **scoped AI Console session** on that ticket, reusing task injection
([[CPE-313]]). On dispatch, an **agent chooser** defaults to the last-used agent/provider/model.

## Acceptance Criteria
- [x] A card has a **Dispatch** action; it opens an agent chooser prefilled with the last-used choice.
      *(The AI Console launcher IS the chooser and already defaults to last-used via `applyLastUsed`;
      Dispatch opens it scoped + task-injected.)*
- [x] Confirming launches an AI Console session scoped to the ticket (its content injected as the task
      via CPE-313), and moves the card to Doing.
- [x] The launched session is correlated to the card (session chip / ticket id) so it's traceable.
      *(The injected task names the ticket id — lightweight correlation; a tighter chip↔card link is a
      CPE-523 follow-on.)*
- [x] Never auto-launches from a plain drag (explicit action only).
- [x] Tests for the dispatch payload (ticket → task injection) + last-used default.

## Resolution
Added an explicit **Dispatch** action to each board card, reusing the CPE-313 explorer→console hand-off.

- **Pure `ticketTask(card)`** (`board.ts`, tested): builds the injected task string from a card
  (`Work on ticket <id>: <title>`, or `Work on ticket <id>.` for a blank title).
- **`BoardView` Dispatch button** (appears on card hover, `|stopPropagation` so it doesn't start a
  drag): `dispatchCard` moves the card to **Doing** (`board_move`, optimistic + reload) then emits a
  `launch { id, task }` event. **Never fires from a drag** — explicit action only (activation decision).
- **App wiring:** `on:launch` calls the existing `openAiConsole({ cwd: currentPath, task })`, which opens
  the AI Console **scoped to the folder with the ticket as its task** (CPE-313). The console's own
  launcher is the **agent chooser** and already prefills the **last-used** agent/provider/model
  (`applyLastUsed`) — so the "chooser defaulting to last-used" decision is satisfied without a second UI.

Correlation is via the ticket id carried in the task text; a tighter session-chip↔card link is left to
CPE-523. The live window launch itself is the existing, GUI-verified CPE-313 path. Verified: `npm run
check` clean; 532 frontend tests pass (6 board-model tests incl. `ticketTask`). First ticket of SPR-04.

## Work Log
2026-07-16 — Picked up (SPR-04 wave 2; prereq CPE-520/521 + CPE-313). Estimate: 3-4h.
2026-07-16 — Added ticketTask (pure, tested), a card Dispatch button → move-to-Doing + launch event, and App wiring to openAiConsole (scoped + task-injected; launcher supplies the last-used chooser). npm check clean; 532 tests pass. All ACs met (live launch = existing CPE-313 path).

## Notes
**needs-prereq:** [[CPE-520]]/[[CPE-521]] (board) + [[CPE-313]] (task injection). Explicit-dispatch +
chooser per the activation decision.
