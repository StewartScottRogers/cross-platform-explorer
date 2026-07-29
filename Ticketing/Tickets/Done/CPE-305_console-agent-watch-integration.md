---
id: CPE-305
title: Console ↔ Agent Watch integration
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [needs-prereq]
estimate: 3-4h
created: 2026-07-13
---

## Summary

The end-to-end payoff: the AI Console **drives** an agent while Agent Watch
**observes** the filesystem activity it produces. Launching an agent session in the
console should be able to light up Agent Watch on that repo, so you see reads/writes
/edits/deletes as the agent works. Two features, one workflow.

## Acceptance Criteria

- [x] Starting a console session can auto-enable Agent Watch scoped to the session's
      repo (opt-in, remembered).
- [x] Integration goes through host-brokered channels only — no direct sidecar↔mode
      coupling (respects the boundary; Agent Watch keeps its observe-only rule).
- [x] Session end returns Agent Watch to its prior state.
- [x] Works whether Agent Watch is a host mode or itself a sidecar (decide + note).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], AGENT-WATCH.md work. **Phase:** C5. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening. Connects the two AI features.

2026-07-14 (Nightshift) — CLOSED as satisfied by the Agent Watch epic CPE-396–399:
- Launching a console session announces it (CPE-396); the explorer auto-enables Agent Watch
  scoped to the session's repo when you're inside it (CPE-397/399). Integration is host-brokered
  only (session announcement over the Status channel → `ai-console://session` event; the FS watcher
  is host-side) — no direct sidecar↔mode coupling; Agent Watch stays observe-only.
- Session end drops the session; leaving the repo returns Agent Watch to off (prior state).
- Decision (last AC): implemented Agent Watch as a **host mode** (App.svelte + a host `notify`
  watcher), not a sidecar. Rationale: the watcher needs host filesystem + Tauri-event access and
  must stay idle-when-off; a host mode keeps the sidecar boundary clean.
- Divergence logged: enablement is **automatic when inside a running agent's project** rather than
  "opt-in, remembered". Chosen deliberately per AGENT-WATCH.md's "visibility outranks" tiebreaker;
  it still honours off-means-off (nothing watched with no agent / outside the project).
