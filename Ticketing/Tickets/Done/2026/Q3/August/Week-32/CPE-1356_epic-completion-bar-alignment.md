---
id: CPE-1356
title: "Agent Board Epics view: epic completion bars are wrong (0/0 empty on Done epics; full bar on in-progress; misaligned)"
type: Bug
status: Done
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-922
created: 2026-08-06
closed: 2026-08-06
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

## Work Log

- 2026-08-06 — Implemented the fix in the in-process board (the only board that renders epic bars):
  - `src/lib/board.ts`: added `epicBar(status, done, total) → {percent, label, state}` (state =
    complete|partial|empty). **Done** epic → complete/100%, label `done/total` or `done` when it has 0
    counted children (archived) — never a misleading `0/0`. Non-Done with 0 children → `empty`/`—`.
    Otherwise `partial` with the honest child percent. `epicProgress` doc clarified to pass the full
    (recent+archived) card set.
  - `src/lib/components/BoardView.svelte`: new `$: epicCountCards = [...cards, ...archived]` (archived
    is loaded unconditionally by `loadArchived()` on mount, so the count is independent of the display
    `showArchived` toggle; the two sets are disjoint so no double-count). Epics view now renders
    `epicBar(e.status, …)` with a state class + label. CSS: `.is-complete` solid accent, `.is-partial`
    hatched (`repeating-linear-gradient`, transparent gaps reveal the grey track — no color-mix), so an
    open epic at 100% child-completion reads as in-progress, not complete; `.is-empty` shows only the
    aligned muted track (no sliver).
  - `src/lib/board.test.ts`: added "epic completion bar (CPE-1356)" block — archived-children counting,
    Done-with-zero-children, not-decomposed empty state, and the in-progress-at-100% partial case.
  - Verify: `board.test.ts` 24/24 green; `npm run check` 0 errors.
- 2026-08-06 — **Sidecar board decision (lockstep, [[two-board-implementations]]):** the sidecar
  Epics view (`sidecar/agent-board/src/ui.rs` `loadEpics`) renders a plain row list (id + title +
  status) with **no completion bars**, and its `Epic` struct carries no done/total — so it has neither
  the bug nor a contradicting completion claim. The two boards already agree (both defer to the epic's
  status/swim-lane); no sidecar change is required for this bug. Adding bars to the sidecar is optional
  future parity, not part of this fix.
- 2026-08-06 — Independent code review dispatched; attended visual re-check on a real build still
  pending (bundled into the next installed build).
- 2026-08-06 — Independent review: **APPROVE** (correct, complete, well-tested; archived-load-order,
  disjointness, hoisting, hatch-vs-solid distinctness all verified). Applied two cosmetic nits from the
  review: fixed the `loadArchived()` comment reference, and simplified the Done/0-children tooltip to
  "Epic complete" (it no longer over-claims "archived" when an epic simply had no sub-tickets). Code
  complete + committed. Attended visual re-check on a real build remains (bundled into the next build).
