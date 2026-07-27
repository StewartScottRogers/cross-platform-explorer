---
id: CPE-1120
title: "Wire agent `sessions` through ExplorerPane → FileList (activate owner-colour sorted-index)"
type: chore
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-26
epic: CPE-730
---

## Summary
Fast-follow to CPE-1116 (owner-coloured heat-map). `FileList.svelte` accepts a `sessions` prop that
`colorForActor` uses to assign each agent a **stable colour by sorted-session index** and to show friendly
agent names in the legend. But `ExplorerPane.svelte` does **not** pass `sessions` into `<FileList>` today, so
in production `sessions` defaults to `[]` → `colorForActor` always uses its deterministic **djb2-hash
fallback** and the legend shows shortened session ids instead of agent names. The feature still works
(distinct, per-render-stable colours), but the *intended* sorted-index + named-legend behaviour is dead until
this is wired. Both the CPE-1116 Reviewer and UAT flagged it.

## Design (buildable, ~1-line-scale)
- Pass the live agent sessions store into `<FileList sessions={…}>` from `ExplorerPane.svelte` (the same
  session list already used elsewhere for `activeWatchCwd`/watch targets — grep `agentSessions`).
- Confirm `colorForActor` then resolves live sessionIds via the sorted-session index (not the hash fallback),
  and the legend renders `friendlyActor` names.

## ⚠ Sequencing
- **Sequence AFTER CPE-1112 (PR #432)** — that PR owns `ExplorerPane.svelte`; wiring `sessions` there before it
  merges would collide. Pick this up once #432 lands.

## Acceptance Criteria
- [ ] `ExplorerPane` passes `sessions` to `FileList`; live agent actors colour by the stable sorted-session
      index (not the hash fallback) and the legend shows agent names.
- [ ] Off-means-off preserved; no new deps; `npm run check` clean; `npm test` green.

## Work Log
2026-07-26 (workshift) — Filed as the CPE-1116 fast-follow (Reviewer + UAT both flagged the unwired `sessions`
prop). Low-pri; sequenced after CPE-1112/#432 (ExplorerPane owner).

2026-07-26 (workshift) — Built (PR #436, merged f5e4979a). Reviewer APPROVE + UAT PASS: App passes `sessions={$agentSessions}` -> ExplorerPane -> FileList -> colorForActor; test proves the sorted-session index reached (asserts --agent-1, unreachable via the djb2 hash which gives --agent-4 for the same id) + legend shows friendlyActor names; off-means-off (empty sessions = pre-fix hash fallback). Completes the CPE-1116 owner-heat-map in production. Residual: visual confirm on installed build w/ live multi-agent session (burndown).
