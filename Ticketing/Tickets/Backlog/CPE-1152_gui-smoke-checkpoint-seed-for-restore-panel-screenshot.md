---
id: CPE-1152
title: "gui-smoke: seed a checkpoint so the restore panel + markers can be screenshotted (Visual-Critic verify)"
type: chore
component: Testing
priority: medium
status: Backlog
tags: ready
created: 2026-07-30
epic: CPE-579
---

## Summary
The Agent-Watch **restore panel + checkpoint markers** (CPE-1126) and their refinements (CPE-1151) are
code-complete, reviewed, and unit/component-tested — but they have **never been seen in a screenshot**, so the
new Visual Critic can't judge the look and the user has to be pulled in. The blocker: gui-smoke's
`seedReplayFixture` writes the replay **journal + baseline** (which makes the scrubber render), but a
checkpoint **marker** needs a real CPE-1123 **snapshot store** entry (`checkpoint_list` reads the snapshot
index), which is a *different* on-disk shape the fixture doesn't create. So the Replay tab renders with a
scrubber but zero markers, and the restore panel is unreachable in the harness.

Close that gap so one `npm test` run captures the restore panel — completing CPE-1148's screenshot loop for
this surface and retiring its Manual-Verification-Debt row.

## Acceptance Criteria
- [ ] gui-smoke can seed at least one **checkpoint** for the same folder + within the seeded replay session's
      time range, so a **marker** renders on the Replay scrubber. Preferred mechanism (worker's judgment):
      drive the app's real `checkpoint_create` via the running webview (a `browser.execute` tauri invoke) so
      it writes the genuine snapshot store; OR extend `wdio.conf.ts#seedReplayFixture` to also write a minimal
      valid snapshot index for the folder. Whatever is chosen, use the REAL read path (`checkpoint_list`) — no
      faked marker.
- [ ] A spec (extend `replay.smoke.ts` or a new `checkpoint-restore.smoke.ts`) drives: open Agent Watch →
      Replay tab → assert a marker is present → click it → assert the restore panel renders the preview
      (counts + drift) → `snap("checkpoint-restore-panel")`; and if a drifted file is staged, arm the confirm
      and `snap("checkpoint-revert-confirm")` so the Visual Critic can see the drift echo. Keep specs
      non-blocking (`continue-on-error`) and their existing assertions intact.
- [ ] Running the real harness produces the new PNG(s) in `.screenshots/`; `npm run check` green; the new
      spec passes (or fails only on the same pre-existing env-drift the other specs do).

## Notes
- Epic CPE-579 (self-maintaining quality infra) — the visual rung for CPE-732's restore UI. Unblocks the
  first *formal* Visual-Critic `VISUAL PASS`/`VISUAL CHANGES` verdict on a real GUI ticket.
- Origin: the P2 deferral in CPE-1126 + the user's GUI-verify. Retires the CPE-1126 row on
  `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`.
- Test-infra only; no app-code change expected (consumes existing `checkpoint_create`/`checkpoint_list`).
