---
id: SPR-05
title: "Sprint 5 — Shared agent memory graph"
status: Active
start: 2026-07-16
end: 2026-07-30
created: 2026-07-16
closed:
---

## Goal
Build the **Shared agent memory graph** epic [[CPE-504]]: a per-project `.agentmemory/` store of linked
markdown notes agents read/write to share context — the graph + recall core, then disk persistence + an
MCP tool surface.

## Tickets
- [x] CPE-524 — Graph store + recall (notes, `[[links]]`, relevance)
- [ ] CPE-525 — `.agentmemory/` persistence + MCP tool surface

Order: CPE-524 (store) → CPE-525 (persistence + MCP).
