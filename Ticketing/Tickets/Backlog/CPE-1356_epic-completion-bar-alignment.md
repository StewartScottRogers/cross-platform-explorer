---
id: CPE-1356
title: "Agent Board Epics view: epic completion bars are wrong (0/0 empty on Done epics; full bar on in-progress; misaligned)"
type: Bug
status: Backlog
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-922
created: 2026-08-06
closed:
---

## Problem (observed in the running v0.57.50 sidecar, Agent Board → Epics view)

The per-epic **completion bars** in the Epics kanban do not correspond to the epic's actual
state / swim lane. Concretely, from a live screenshot:

- **Done epics show an EMPTY bar at `0/0`.** e.g. CPE-703 (Instant index search), CPE-704, CPE-707,
  CPE-711, CPE-715, CPE-717 all sit in the **DONE** column but render a `0/0` bar with ~0% fill — a
  *completed* epic reads as 0% complete. Only a few Done epics (CPE-705 2/2, CPE-714 1/1, CPE-718 2/2)
  show a filled bar.
- **An in-progress epic shows a FULL (100%) bar.** CPE-547 (Busy/wait cursor) is in **DOING** but
  renders `3/3` completely filled — the bar says 100% while the epic is still In Progress. The
  child-completion bar and the epic's own swim lane visually contradict.
- **`0/0` bars render as a thin, misaligned sliver** inside the card — looks broken/unaligned rather
  than an intentional "empty" state.

## Root cause

`src/lib/board.ts` `epicProgress(cards, epicId)` (~line 108-110) computes `{ done, total }` purely from
the **loaded `cards` that carry `epic: <id>` frontmatter**:
```ts
const mine = cards.filter((c) => (c.epic || NO_EPIC) === epicId);
return { done: mine.filter((c) => c.column === "Done").length, total: mine.length };
```
So `total === 0` (→ empty bar) whenever an epic's children (a) were never decomposed into child tickets,
(b) don't carry the `epic:` link, or (c) are **archived Done tickets not in the loaded `cards` set**
(a closed epic's children move to `Done/<dated>/…` and aren't loaded by default). That's why most Done
epics show `0/0`. Separately, the bar reflects **child completion**, which is a *different signal* from
the **epic's own `status:`** (its swim lane) — so a 100%-children epic can still be In Progress, and a
Done epic can have 0 counted children.

## Fix direction

1. **Count archived Done children** for closed/Done epics (mirror `doneWithArchived`/`archivedEpics`
   handling already used elsewhere in `board.ts`/`BoardView.svelte`) so a Done epic's `total` isn't 0.
2. **Make the bar consistent with the epic's swim lane / status:**
   - A **Done** epic should read as complete: show a full bar (and, if it genuinely has 0 counted
     children, treat it as 100% or show a distinct "no sub-tickets" state — NOT an empty 0% bar).
   - A **Backlog/Proposed** epic (not decomposed, legitimately `0/0`) should render a clear **empty /
     "not yet decomposed"** state, not a broken sliver.
   - Decide the intended meaning when child-completion (e.g. 3/3) disagrees with epic status (still
     Doing): either drive the bar from status, or show both without them contradicting.
3. **Fix the bar alignment/geometry** so the `0/0` (empty) and full states align within the card and
   read as the same control (consistent height/inset/track), per the app's visual conventions.
4. Add/adjust unit tests in `src/lib/board.test.ts` for `epicProgress` (archived children counted;
   Done-epic-with-zero-children case; not-decomposed case).

## Lockstep — two board implementations (see [[two-board-implementations]])

The board exists **twice**: the in-process `src/lib/components/BoardView.svelte` + `src/lib/board.ts`
(the screenshot — tab set Board/Epics/Project/Docs), and the standalone **sidecar** board in
`sidecar/agent-board/src/{ui.rs,board.rs}`. Both read the same `Ticketing/` folders and must present the
epic progress consistently — apply the fix (and the archived-children counting) to BOTH, or share one
source of truth. The sidecar's Epics view (`ui.rs` `loadEpics`) currently renders a simpler row list;
bring it to parity if it is to show bars.

## Acceptance criteria

- Done epics no longer show an empty `0/0` bar; their bar reads as complete (incl. when children are
  archived). In-progress epics don't show a misleading 100% bar that contradicts their swim lane.
- `0/0` not-yet-decomposed epics render a clear, aligned empty state (no thin misaligned sliver).
- `epicProgress` unit tests cover archived children + the zero-children Done/Proposed cases; the bar
  geometry is consistent across empty/partial/full.
- In-process and sidecar boards agree. `npm run check` + JS suite green; any Rust green.
- Attended visual re-check on a real build (the completion bars look right per swim lane).

## Notes

Filed 2026-08-06 from the running v0.57.50-sidecar Agent Board (user-reported). Epic CPE-922
(Epics-as-kanban). The final "looks right" is an attended visual check.
