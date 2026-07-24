---
id: CPE-960
title: Card-detail popup — resizable + statusbar + thumb, pop out to its own window, fix epic drill
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
closed: 2026-07-23
created: 2026-07-23
epic: CPE-503
---

## Summary
Enhance the Agent Board card-detail popup (CPE-959): make it **resizable** (corner thumb + a **statusbar**),
add a **pop-out** to a standalone window (outside the board dialog), and fix a bug where an **epic's "View
tickets"** could show an empty board.

## Acceptance Criteria
- [x] Fix: `drillEpic` now also sets `showArchived = true`, so an epic's "View tickets →" reveals its
      done/archived children (`Done/**`) instead of a possibly-empty board.
- [x] Embedded card popup is **resizable** by a bottom-right **thumb**; size clamped to the viewport +
      persisted (`cpe.cardDetailSize`).
- [x] **Statusbar** along the bottom: location + field/line counts, with the resize thumb in its corner.
- [x] **Pop-out** button opens the card in its **own window** (`index.html?card=<id>&root=<path>`) via a new
      `bootMode` "card" + standalone `AgentCardApp`; the standalone `CardDetailDialog` fills the frame (no
      backdrop/thumb/pop-out), close = close window.
- [x] `capabilities/default.json` `windows` now includes `card-detail-*` (invoke `board_card_detail`).
- [x] `npm run check` 0/0; vitest **930 pass** (incl. new `bootMode` "card" test); app `cargo check` clean
      (capabilities valid).
- [ ] GUI-verify: resize + pop-out the popup, and epic "View tickets" shows tickets. *(attended, together)*

## Notes
Mirror the `?board` window (`AgentBoardApp` + `bootMode` + `capabilities.windows`). Reuse the board resize
helpers style (`board-grip`, clamp, persist). `CardDetailDialog` gains a `standalone` prop.
