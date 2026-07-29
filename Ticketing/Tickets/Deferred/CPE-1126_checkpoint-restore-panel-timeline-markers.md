---
id: CPE-1126
title: "Checkpoint & rollback: restore panel + timeline checkpoint markers (GUI cap)"
type: feature
component: Frontend
priority: medium
status: Deferred
tags: needs-gui-verify
created: 2026-07-26
epic: CPE-732
---

## Summary
CPE-732's **attended GUI cap** (~15% of the epic). The visual restore experience: a **restore panel** that shows
a checkpoint's revert plan + drift warning for a person to review before reverting, and **checkpoint markers** on
the Agent-Watch timeline. The command-level flow (create/list/preview/revert) ships headlessly via CPE-1125's
palette; this ticket is the visual review layer that genuinely needs human eyes.

## Why Deferred
Building it is fine headlessly, but its VALUE (does the plan read clearly? do markers land right? is the
revert-confirm UX safe?) can only be verified with the user present on the installed build (build → deploy → run,
with a real watched session + checkpoints). Per the workshift skip-and-note escalation, this is deferred to a
GUI-verification session rather than faked. It is on the QA Manual-Verification-Debt ledger.

## Acceptance Criteria (when picked up with the user present)
- [ ] A restore panel renders the CPE-1123 `checkpoint_preview_revert` plan + drift warning; confirm-to-revert is
      safe/clear; timeline shows checkpoint markers. Theme vars only; reflow; off-means-off.

## Work Log
2026-07-26 (workshift) — Filed as the CPE-732 deferred GUI cap (PM analysis). Backend + palette + e2e tests ship
headlessly this shift (CPE-1123/1124/1125); this visual layer waits for a user-present GUI session.
