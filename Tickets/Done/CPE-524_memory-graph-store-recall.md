---
id: CPE-524
title: "Shared memory — graph store + recall (notes, [[links]], relevance)"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [ready]
epic: CPE-504
sprint: SPR-05
closed: 2026-07-16
estimate: 3-4h
created: 2026-07-16
---

## Summary
Core of the shared agent memory ([[CPE-504]]). A per-project store of **markdown notes** (frontmatter +
`[[wikilinks]]`) forming a **graph**; add/get/list notes, resolve links to **neighbors**, and **recall**
the most relevant notes for a query (tag overlap + link proximity + text match). Pure + unit-tested;
**separate** from the app's own `memory/MEMORY.md` (activation decision) but reusing its shape.

## Acceptance Criteria
- [x] A note model (id/slug, tags, `[[links]]`, body) parses from markdown; malformed ⇒ skipped, no panic.
- [x] Build the link graph + `neighbors(id)`; a `[[link]]` to a missing note is tolerated (dangling).
- [x] `recall(query, tags)` ranks notes by relevance (tag overlap + link proximity + text match), top-N.
- [x] Append-only + content-hash **dedup** so concurrent writers don't create duplicates or conflicts.
- [x] Pure core, comprehensively unit-tested (parse, graph, recall ranking, dedup).

## Notes
Wave 1 of [[CPE-504]]. Backs the MCP tools (CPE-525). Separate store per the activation decision.

## Resolution
Added `sidecar/ai-console/src/agent_memory.rs` (new, pure, 6 tests) — the shared-memory graph core.

- **`Note`** = id + tags + `[[links]]` + body; `parse_note(md)` reads frontmatter (`id`/`tags`) + `parse_links` extracts `[[targets]]` from the body (no id ⇒ None, tolerant).
- **`MemoryGraph`**: `add` is **append-only with content-hash dedup** (identical tags+body skipped) so concurrent writers never conflict; `get`, `neighbors` (resolves links, tolerates dangling), `backlinks`.
- **`recall(query, tags, n)`** ranks by **tag overlap (×3) + text-term matches + link proximity** (half the best linked neighbour's base score); only positive scores, best-first, top-N.

Separate from the app's own `memory/MEMORY.md` (activation decision). clippy clean; 6 unit tests (parse+links, no-id, dedup, neighbors/backlinks, recall ranking, empty recall). First ticket of SPR-05.

## Work Log
2026-07-16 — Picked up (SPR-05). Built the pure memory graph (Note/parse/MemoryGraph/recall) with 6 tests. clippy clean. All ACs met.
