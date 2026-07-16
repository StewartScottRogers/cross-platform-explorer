---
id: CPE-504
title: "EPIC: Shared agent memory graph over MCP"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-16
---

## Summary
Part of the **Agent Workspace** program (sibling to the AI Console [[CPE-261]]; from spike [[CPE-500]]).
A **shared, per-project markdown memory graph** every agent reads/writes **over MCP** — the BridgeMemory
analogue. What one agent learns, the next starts with. A brief only until activated.

## Goal
Persist cross-agent knowledge as a linked markdown graph (a `.*memory/`-style store) exposed to every
agent via MCP, so a swarm (or successive sessions) share context instead of re-deriving it.

## Rough scope (NOT decomposed)
- Markdown memory files + a link graph, per-project, on disk.
- MCP read/write tools (reuse the AI Console MCP system CPE-288/307) the agents call.
- Relevance/recall + dedup; concurrent-write reconciliation.

## Open questions (resolve at activation)
- Relationship to this app's *own* auto-memory (`memory/MEMORY.md`) — share a model or keep separate?
- Graph/link format; how memories are recalled into an agent's context.
- Conflict/merge when multiple swarm agents write at once.

## Notes
From [[CPE-500]]; builds on the AI Console MCP plumbing.
