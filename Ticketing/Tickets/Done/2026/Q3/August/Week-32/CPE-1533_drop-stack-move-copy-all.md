---
id: CPE-1533
title: "Drop Stack: Move-all / Copy-all into the active folder via the transfer queue"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1489
parent: CPE-1489
created: 2026-08-09
---
## Context
This is the payoff ticket: turning the accumulated Drop Stack into a real, tracked filesystem operation.
"Move all" / "Copy all" takes every item currently on the stack (regardless of which folder each was
added from — that's the whole point of the feature) and moves/copies it into the folder the user is
looking at right now, through the **existing** transfer engine (CPE-613) so it gets progress, conflict
handling, and (for move) undo for free — no new backend surface.

## Scope
- Two buttons in `DropStackPanel.svelte` (from CPE-1532): "Move all here" / "Copy all here", operating on
  the full current stack against `currentPath`.
- Reuses the existing move/copy call paths already used by paste (`commands.moveEntries` /
  `commands.copyEntries`, see `src/App.svelte` around `doPaste`/`startCopyWithPolicy`, ~line 3316 on) —
  same collision handling (CPE-624 conflict dialog) rather than a new one-off path.
- On successful completion, clear the moved/copied items from the Drop Stack (partial failure: only clear
  the items that actually transferred — mirror how the existing paste flow reports `TransferReport`
  skipped/failed).
- Disabled state when the stack is empty or the destination folder isn't a valid paste target (mirror
  `doPaste`'s existing `isHome`/archive-blocked guards).
- No new backend command — this is purely wiring existing frontend transfer plumbing to a new source
  (the Drop Stack) instead of the clipboard.

## How
- Add the two handlers in `src/App.svelte` (near `doPaste`/`startCopyWithPolicy`), reading source paths
  from CPE-1530's `dropStack` store instead of `clipboard`, targeting `currentPath`.
- Wire the two buttons in `DropStackPanel.svelte` to call these handlers (passed in as props or imported,
  matching how the panel already receives its data).
- On the `transfer://done` report (via `src/lib/transfers.ts`, already wired), remove the
  successfully-transferred paths from the Drop Stack store.

## Verify
`npm run check` + `npx vitest run` extending `DropStackPanel.test.ts` (button click invokes the passed
handler) and a new/extended App-level test (mirror `App.clipboardPaneRouting.test.ts`'s style) asserting:
Move-all calls `commands.moveEntries` with the full stack's paths and `currentPath`; Copy-all calls
`commands.copyEntries` similarly; a completed report clears only the transferred paths from the stack;
disabled when stack is empty. Mock the Tauri commands the same way existing paste tests do — fully
headless, no real filesystem or OS involved.

## Notes
**Conflict surface:** `src/lib/components/DropStackPanel.svelte` (adding the two buttons — same file as
CPE-1532, so this ticket must **not** start until CPE-1532 has landed) and `src/App.svelte` (two new
handler functions near `doPaste`, additive — same caution as CPE-1531 about App.svelte contention).
**Dispatch order: after CPE-1530 AND CPE-1532** (last in the chain: 1530 → {1531 ∥ 1532} → 1533).
