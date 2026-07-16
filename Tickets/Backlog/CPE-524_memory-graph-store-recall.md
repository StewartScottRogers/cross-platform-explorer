---
id: CPE-524
title: "Shared memory — graph store + recall (notes, [[links]], relevance)"
type: Feature
status: Open
priority: Medium
component: Sidecar
tags: [ready]
epic: CPE-504
sprint: SPR-05
estimate: 3-4h
created: 2026-07-16
---

## Summary
Core of the shared agent memory ([[CPE-504]]). A per-project store of **markdown notes** (frontmatter +
`[[wikilinks]]`) forming a **graph**; add/get/list notes, resolve links to **neighbors**, and **recall**
the most relevant notes for a query (tag overlap + link proximity + text match). Pure + unit-tested;
**separate** from the app's own `memory/MEMORY.md` (activation decision) but reusing its shape.

## Acceptance Criteria
- [ ] A note model (id/slug, tags, `[[links]]`, body) parses from markdown; malformed ⇒ skipped, no panic.
- [ ] Build the link graph + `neighbors(id)`; a `[[link]]` to a missing note is tolerated (dangling).
- [ ] `recall(query, tags)` ranks notes by relevance (tag overlap + link proximity + text match), top-N.
- [ ] Append-only + content-hash **dedup** so concurrent writers don't create duplicates or conflicts.
- [ ] Pure core, comprehensively unit-tested (parse, graph, recall ranking, dedup).

## Notes
Wave 1 of [[CPE-504]]. Backs the MCP tools (CPE-525). Separate store per the activation decision.
