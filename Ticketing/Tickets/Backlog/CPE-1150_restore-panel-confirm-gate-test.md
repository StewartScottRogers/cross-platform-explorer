---
id: CPE-1150
title: "Component test: restore-panel two-step revert confirm gate (AgentTimeline)"
type: chore
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-07-30
epic: CPE-732
---

## Summary
Follow-up from the independent review of CPE-1126 (PR #466). The new **restore panel** on the Agent-Watch
Replay tab can trigger a filesystem revert that overwrites/deletes files (no Recycle Bin). Its safety rests on
a **two-step confirm** (the "Revert to this checkpoint…" button only *arms* a confirm; the actual
`commands.checkpointRevert(...)` fires only from a separate "Yes, revert" click). The reviewer verified this by
reading the code and noted it mirrors the already-tested `CheckpointDialog`, but there is **zero automated
component-level coverage** of this gate in `AgentTimeline.test.ts` — all of CPE-1126's new tests cover the pure
`checkpointMarkers` helper. Given how destructive the action is, the gate deserves a regression test.

## Acceptance Criteria
- [x] A component test in `src/lib/components/AgentTimeline.test.ts` (or a focused sibling) asserts, with a
      mocked `commands`:
  - clicking "Revert to this checkpoint…" does **NOT** call `checkpointRevert` (only arms the confirm);
  - `checkpointRevert` is called **only** after the subsequent "Yes, revert" click, with the expected
    `(currentPath, manifest_id)` args;
  - "Cancel" in the confirm dismisses it and leaves `checkpointRevert` **uncalled**;
  - the destructive action is disabled / no-ops when `currentPath` is empty.
- [x] (If cheap) assert the confirm message names the selected checkpoint and includes the "cannot be undone"
      wording, so a future refactor can't silently drop the warning.
- [x] `npm run check` green; the new test(s) pass; no existing assertion weakened.

## Work Log
- 2026-07-30 (worker, branch `cpe-1150-confirm-gate-test`): Added a new describe block
  **"AgentTimeline checkpoint restore panel — two-step revert confirm gate (CPE-1150, epic CPE-732)"**
  to `src/lib/components/AgentTimeline.test.ts` (5 tests), reusing the file's existing harness
  (`entry`, the `@tauri-apps/api/core` `invokeMock`, and `flushReplayLoad`). The tests mount the
  drawer, enter the Replay tab, mock `checkpoint_list` → one in-range `Checkpoint`,
  `checkpoint_preview_revert` → a `RevertPreview`, and `checkpoint_revert` → a `RevertOutcome`, click
  the scrubber marker to open the restore panel, then exercise the gate:
  1. `arming: …` — clicking `checkpoint-revert-btn` reveals `checkpoint-confirm-revert` but does NOT
     invoke `checkpoint_revert`.
  2. `confirming: …` — `checkpoint_revert` fires only after `checkpoint-confirm-yes`, called with
     `{ root: currentPath, manifestId: manifest_id }`; outcome renders.
  3. `cancelling: …` — `checkpoint-confirm-cancel` dismisses the confirm, revert stays uncalled.
  4. `empty currentPath: …` — with `currentPath=""` no `checkpoint_list`/markers/restore panel appear,
     so the destructive path is unreachable and `checkpoint_revert` is never called.
  5. `confirm message …` — the confirm text contains the checkpoint label, the folder path, and
     matches `/cannot be undone/i`.
- Assumption logged: for AC "disabled/no-op when currentPath empty", the honest component-level
  behavior is that the whole checkpoint layer (list → markers → panel → revert) is gated behind a
  non-empty `currentPath`, so with an empty path the destructive action is *unreachable* rather than a
  rendered-but-disabled button. Test 4 asserts that unreachability directly.
- Avoided `findBy*`/`waitFor` (the file documents them flaking against real-timer restoration after its
  fake-timer tests); used the existing microtask-drain `flushReplayLoad` + synchronous `getByTestId`.
- **No production-code change** — the restore panel already exposes stable `data-testid`s from CPE-1126.
- Verification: `npx vitest run src/lib/components/AgentTimeline.test.ts` → 37 passed (5 new);
  `npm run check` → 0 errors / 0 warnings; full `npx vitest run` → 1415 passed / 126 files, no regression.

## Notes
- Pure frontend/test work — no Rust, no bindings regen. Epic CPE-732 (checkpoint & rollback).
- Origin: CPE-1126 PR #466 reviewer flag #1. Mirrors the confirm pattern already covered for
  `CheckpointDialog` — reuse that test's approach.
