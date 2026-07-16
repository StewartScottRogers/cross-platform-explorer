---
id: CPE-525
title: "Shared memory — on-disk .agentmemory/ persistence + MCP tool surface"
type: Feature
status: Open
priority: Medium
component: Sidecar
tags: [needs-prereq]
epic: CPE-504
sprint: SPR-05
estimate: 2-3h
created: 2026-07-16
---

## Summary
Persist the memory graph ([[CPE-524]]) to a per-project **`.agentmemory/`** folder (one markdown file
per note) and expose **MCP tools** (`memory.write` / `memory.read` / `memory.recall`) so any agent can
share context ([[CPE-504]]). Reuses the AI Console MCP plumbing ([[CPE-288]]/[[CPE-307]]).

## Acceptance Criteria
- [ ] Load a project's `.agentmemory/*.md` into the graph; write a new note as a file (dedup by hash).
- [ ] A defined MCP tool surface — `memory.write{tags,body,links}` / `memory.read{id}` /
      `memory.recall{query}` — mapped onto the store.
- [ ] Writes are append-only files so concurrent swarm agents never conflict.
- [ ] Degrades safely if the folder/MCP is unavailable (clear error, no crash).
- [ ] Tests for the load/save round-trip + the tool-surface mapping (live MCP round-trip flagged).

## Notes
**needs-prereq:** [[CPE-524]] (the store) + the MCP layer. Live MCP-server wiring is the integration
follow-on (as with the Swarm mailbox [[CPE-516]]).
