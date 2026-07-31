---
id: CPE-1168
title: "Automated click-through for the standalone agent-board sidecar UI (retire MVD row #9)"
type: chore
component: Testing
priority: low
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-579
---

## Summary
QA-Architect / PM-scouted (2026-07-31). `MANUAL-TEST-BURNDOWN.md` row #9 — the standalone **`agent-board`
sidecar UI** click-through — is still manual but is **genuinely headless-automatable**: the sidecar serves a
loopback HTTP UI (no user, no creds), so a WebDriver/`gui-smoke`-style drive can launch it and click each
Board / Epics / Sprints view button and assert the list swaps.

## Build
- Add an automated click-through (extend `gui-smoke` or a focused harness) that: launches the `agent-board`
  sidecar (find its bin/launch under `sidecar/agent-board` + how it's normally started; it serves loopback
  HTTP), drives its UI (Chromium/WebDriver against the loopback URL), clicks each top-level view
  (Board/Epics/Sprints), and asserts the rendered list changes per view. Snap a screenshot per view for the
  Visual Critic if cheap.
- Time-bounded + non-blocking; tear the sidecar down after. No user, no network beyond loopback.

## Acceptance Criteria
- [ ] A headless run launches the agent-board sidecar, clicks Board/Epics/Sprints, and asserts each view's
      list renders/swaps; tears the sidecar down cleanly.
- [ ] Flips burndown row #9 to automated + names the pinning job; `npm run check` / relevant checks green.

## Notes
- Lowest priority of this shift's batch — dispatch only if crew budget/disk allows after CPE-1166/1167.
  Epic CPE-579. Two board implementations exist ([[two-board-implementations]]) — this pins the standalone
  sidecar one specifically.
