---
id: CPE-525
title: "Shared memory — on-disk .agentmemory/ persistence + MCP tool surface"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [needs-prereq]
epic: CPE-504
sprint: SPR-05
closed: 2026-07-16
estimate: 2-3h
created: 2026-07-16
---

## Summary
Persist the memory graph ([[CPE-524]]) to a per-project **`.agentmemory/`** folder (one markdown file
per note) and expose **MCP tools** (`memory.write` / `memory.read` / `memory.recall`) so any agent can
share context ([[CPE-504]]). Reuses the AI Console MCP plumbing ([[CPE-288]]/[[CPE-307]]).

## Acceptance Criteria
- [x] Load a project's `.agentmemory/*.md` into the graph; write a new note as a file (dedup by hash).
- [x] A defined MCP tool surface — `memory.write{tags,body,links}` / `memory.read{id}` /
      `memory.recall{query}` — mapped onto the store.
- [x] Writes are append-only files so concurrent swarm agents never conflict.
- [x] Degrades safely if the folder/MCP is unavailable (clear error, no crash).
- [x] Tests for the load/save round-trip + the tool-surface mapping (live MCP round-trip flagged).

## Notes
**needs-prereq:** [[CPE-524]] (the store) + the MCP layer. Live MCP-server wiring is the integration
follow-on (as with the Swarm mailbox [[CPE-516]]).

## Resolution
Extended `agent_memory.rs` with disk persistence + the MCP tool surface.

- **Persistence:** `note_to_markdown` (round-trips with `parse_note`), `save_note(dir, note)` writes
  `<id>-<hash8>.md` into `.agentmemory/` (created if needed) — **append-only**, the hash suffix keeps
  concurrent writers in distinct files (never clobbering); `load_dir(dir)` reads every `*.md` into the
  graph (dedup applies; a **missing dir ⇒ empty graph**, not an error).
- **MCP tool surface (pure adapter):** `memory_tool(graph, tool, args)` maps `memory.write` /
  `memory.read` / `memory.recall` onto the store, returning JSON — `write` dedups + returns `stored`,
  `read` returns the note, `recall` returns ranked notes; unknown tool / empty body ⇒ `ok:false`.
- **Live MCP-server registration** (exposing these tools to external agent processes over the MCP layer)
  is the integration follow-on, as with the Swarm mailbox (CPE-516) — the pure adapter + persistence are
  fully tested here.

clippy clean; 10 agent_memory tests (4 new: markdown round-trip, save/load round-trip incl dedup,
missing-dir, tool write/read/recall). Second ticket of SPR-05 — **completes the Shared-memory epic CPE-504**.

## Work Log
2026-07-16 — Picked up (SPR-05; prereq CPE-524). Added note_to_markdown/save_note/load_dir (append-only, hash-suffixed, temp-dir tested) + memory_tool dispatch (write/read/recall, pure). 4 new tests. clippy clean. Live MCP-server wiring flagged. All ACs met.
