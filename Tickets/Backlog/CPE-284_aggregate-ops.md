---
id: CPE-284
title: Aggregate ops (install / update / uninstall all)
type: Feature
status: Open
priority: Low
component: Backend
estimate: 1-2h
created: 2026-07-13
---

## Summary

Port `Install-All` / `Update-All` / `Uninstall-All`: run a lifecycle action across
every registered (or selected) agent, sequentially, with per-agent progress and a
summary — one click to set up or refresh a whole toolbox.

## Acceptance Criteria

- [ ] Run install/update/uninstall across all or a chosen subset of agents.
- [ ] Per-agent progress + a final success/failure summary; one failure doesn't
      abort the rest.
- [ ] Reuses the single-agent lifecycle commands ([[CPE-282]]/[[CPE-283]]).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-282]], [[CPE-283]]. **Phase:** C3. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
