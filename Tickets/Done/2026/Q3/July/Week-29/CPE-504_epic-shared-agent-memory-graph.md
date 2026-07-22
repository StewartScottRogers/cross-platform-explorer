---
id: CPE-504
title: "EPIC: Shared agent memory graph over MCP"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-16
closed: 2026-07-16
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

## Decisions (activation 2026-07-16)
Adopted on the user's "just do them all" directive — recommended defaults, reversible:
- **Relationship to the app's own memory:** **separate store** — a per-project `.agentmemory/` for
  agents, distinct from the app's `memory/MEMORY.md`, but reusing its shape (markdown + frontmatter +
  `[[links]]`).
- **Format/recall:** markdown notes forming a link graph; **recall** ranks by tag overlap + link
  proximity + text match.
- **Conflict:** **append-only** (one file per note) + content-hash **dedup** — concurrent swarm writers
  never merge-conflict.

## Child tickets (created at activation)
Sprint **[[SPR-05]]**:
- [[CPE-524]] — Graph store + recall (notes, `[[links]]`, relevance) *(ready)*
- [[CPE-525]] — `.agentmemory/` persistence + MCP tool surface *(needs-prereq CPE-524; live MCP flagged)*

## Resolution (closed 2026-07-16)
Shared agent memory shipped across 2 children in SPR-05: [[CPE-524]] the pure graph store + recall
(markdown notes, `[[links]]`, relevance by tag/text/link-proximity, append-only + hash dedup) and
[[CPE-525]] `.agentmemory/` disk persistence + the `memory.write/read/recall` MCP tool adapter. Agents
can now write notes and recall relevant prior context — separate from the app's own memory. 10 unit
tests; clippy clean. **Follow-on (recorded):** live MCP-server registration exposing the tools to
external agent processes (as with the Swarm mailbox CPE-516) — the core is proven in isolation.

## Notes
From [[CPE-500]]; builds on the AI Console MCP plumbing.

## Work Log
2026-07-16 — Filed as a dormant `Proposed` brief (from spike CPE-500).
2026-07-16 — **Activated** into SPR-05 on the "do them all" directive (defaults recorded above).
Decomposed into CPE-524 (store+recall) + CPE-525 (persistence+MCP). Status → In Progress.
