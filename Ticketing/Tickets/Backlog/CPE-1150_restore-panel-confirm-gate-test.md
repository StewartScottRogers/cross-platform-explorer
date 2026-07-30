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
- [ ] A component test in `src/lib/components/AgentTimeline.test.ts` (or a focused sibling) asserts, with a
      mocked `commands`:
  - clicking "Revert to this checkpoint…" does **NOT** call `checkpointRevert` (only arms the confirm);
  - `checkpointRevert` is called **only** after the subsequent "Yes, revert" click, with the expected
    `(currentPath, manifest_id)` args;
  - "Cancel" in the confirm dismisses it and leaves `checkpointRevert` **uncalled**;
  - the destructive action is disabled / no-ops when `currentPath` is empty.
- [ ] (If cheap) assert the confirm message names the selected checkpoint and includes the "cannot be undone"
      wording, so a future refactor can't silently drop the warning.
- [ ] `npm run check` green; the new test(s) pass; no existing assertion weakened.

## Notes
- Pure frontend/test work — no Rust, no bindings regen. Epic CPE-732 (checkpoint & rollback).
- Origin: CPE-1126 PR #466 reviewer flag #1. Mirrors the confirm pattern already covered for
  `CheckpointDialog` — reuse that test's approach.
