---
id: CPE-1538
title: "Drop Stack Move-all: add the doPaste-style double-click re-entrancy guard (CPE-1385 parity)"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-1489
parent: CPE-1533
created: 2026-08-09
---
## Why (independent reviewer note on the merged PR #758 / CPE-1533, non-blocking)
`doDropStackMoveAll` (src/App.svelte) mirrors `doPaste`'s cut/move branch but does **not** replicate
`doPaste`'s CPE-1385 re-entrancy fix: `doPaste` synchronously *claims/clears* its source set (the clipboard)
**before** the `await`, so a fast double-click can't re-read the same sources and fire a second concurrent
operation. `doDropStackMoveAll` reads `$dropStackEntries` and does NOT synchronously clear/claim them before
`await commands.moveEntries(...)`, so a very fast double-click on **"Move all here"** could fire two
concurrent `moveEntries` calls with the same paths.

**Worst case observed (not data loss):** the second call's items fail because the source was already moved,
and the user sees a spurious "N failed" notice. Drop Stack state stays correct (only genuinely-`ok` results are
ever cleared). So this is a UX papercut, not a correctness/data bug — hence it did not block CPE-1533.

## Scope (small, frontend-only)
- Give `doDropStackMoveAll` (and consider `doDropStackCopyAll`) the same **synchronous claim-before-await**
  guard `doPaste` uses for CPE-1385: capture the source set and set an in-flight flag (or clear/snapshot the
  shelved paths) synchronously before the first `await`, so a second click within the same tick is a no-op or
  operates on an already-emptied set.
- Follow `doPaste`'s exact pattern for consistency (grep CPE-1385 in App.svelte + the
  `App.clipboardPaneRouting.test.ts` re-entrancy tests).

## Verify
- Unit test: two synchronous "Move all here" dispatches result in exactly ONE `moveEntries` call (mirrors the
  CPE-1385 double-fire guard test). `npm run check` + vitest green.

## Notes
Follow-up to the merged CPE-1533 (Drop Stack move/copy-all). Same epic (CPE-1489). Good small batched-run ticket.
