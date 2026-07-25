---
id: CPE-540
title: "Swarm → session launch-spec bridge (the pure seam of live wiring)"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [ready]
epic: CPE-528
sprint: SPR-09
estimate: 1-2h
created: 2026-07-16
closed: 2026-07-16
---

## Summary
Wave 1 of [[CPE-528]]: the one part of the live wiring that is unit-testable without a running sidecar —
mapping a coordinator `Assignment` ([[CPE-517]]) to a concrete session **launch spec** (base agent,
model from the team, task text).

## Resolution
Added `sidecar/ai-console/src/swarm_bridge.rs` (pure, 4 tests): `launch_spec_for(agent_instance, team,
task) -> SwarmLaunch { agent, model, task }` — parses the base agent + role from the instance id
(`claude#builder1`), looks up the role's model in the team manifest (role disambiguates a shared agent),
and carries the task. clippy clean; 4 tests. The live driver that turns a `SwarmLaunch` into a real
Agent-Grid session (+ reports its result back to the coordinator) is [[CPE-541]] — it needs the running
app + GUI QA. First ticket of SPR-09.

## Work Log
2026-07-16 — Built the pure Assignment→SwarmLaunch mapper with 4 tests. clippy clean. All ACs met.
