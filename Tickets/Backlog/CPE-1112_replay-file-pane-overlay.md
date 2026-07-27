---
id: CPE-1112
title: "Activity replay: read-only file-pane overlay while scrubbing (optional)"
type: feature
component: Frontend
priority: low
status: Backlog
tags: ready
created: 2026-07-26
epic: CPE-728
---

## Summary
OPTIONAL enhancement (CPE-728 slice e — NOT required by the closed epic's DoD, which is satisfied by the
in-drawer reconstruction from CPE-1111). Graduate the reconstructed listing from the Replay-tab drawer to a
**read-only overlay of the MAIN explorer file pane** while scrubbing, so the whole browser shows the folder as
it looked at time T. Highest-risk slice — coexists with a live session still emitting events. Design:
`.claude/research-library/entries/activity-replay-event-reconstruction-plan.md` (§2 "graduate", §4 slice e).

## Design (buildable)
Drive `ExplorerPane`/`FileList` to render `childrenAt(stateAtFrom(baseline, events, t), currentPath)` (the same
`replayFold.ts` fold CPE-1111 already uses) as a **read-only, ephemeral** overlay, gated behind an explicit
**Replay mode** toggle so the live listing and the reconstruction never render simultaneously. Restore the live
listing on exit. Must NOT mutate the live navigation/listing store.

## ⚠ Guardrails / risks
- Strictly read-only + ephemeral; explicit Replay-mode gate; guaranteed restore-on-exit; never mutate live
  `entries`/navigation. Off-means-off; no new deps. This is the risky coexistence slice — de-risk by keeping
  the in-drawer view (CPE-1111) as the always-available fallback.

## Acceptance Criteria
- [ ] A Replay-mode toggle shows the reconstructed listing in the main file pane while scrubbing; exiting
      restores the live listing; live store/navigation never mutated; read-only.
- [ ] `npm run check` clean; vitest green; no new deps; off-means-off.

## Work Log
2026-07-26 (workshift) — Filed as the optional CPE-728 graduate (file-pane overlay). The epic closed on the
in-drawer reconstruction (CPE-1111); this is a nice-to-have, pickable anytime.
