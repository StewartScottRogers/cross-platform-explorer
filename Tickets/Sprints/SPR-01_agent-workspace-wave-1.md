---
id: SPR-01
title: "Sprint 1 — Agent Workspace, wave 1 (Swarm)"
status: Active
start: 2026-07-16
end: 2026-07-30
created: 2026-07-16
closed:
---

## Goal
Land the first wave of the **Agent Workspace** program (from spike [[CPE-500]]): activate the **Swarm
orchestration** epic [[CPE-502]] and work its first children through to Done. Builds directly on the
just-shipped Agent Grid ([[CPE-501]]) and is informed by the Herdr spike ([[CPE-511]]) — its socket
control-API idea is the coordination substrate Swarm needs.

## Tickets
Wave-1 foundation of the Swarm epic [[CPE-502]] (each carries `sprint: SPR-01`):
- [x] CPE-514 — Swarm file-ownership lock manager (path-glob claims + shared-dep sequencing) *(Done)*
- [x] CPE-515 — Swarm role/team manifest model (coordinator/builder/scout/reviewer)
- [x] CPE-516 — Swarm inter-agent mailbox over MCP

_Wave 2 (CPE-517 coordinator, CPE-518 gates, CPE-519 budgets/authority) is queued for a later sprint._
