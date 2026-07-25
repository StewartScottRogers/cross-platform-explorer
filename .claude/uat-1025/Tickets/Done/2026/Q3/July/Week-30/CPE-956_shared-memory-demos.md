---
id: CPE-956
title: Agent Deck — Shared Memory demos (+ more messaging demos) and seed the memory graph
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
created: 2026-07-23
closed: 2026-07-23
epic: CPE-711
---

## Summary
The Agent Deck "Swarm coordination" panel has TWO columns: **Mailbox** (messaging) and **Shared memory**
(a notes graph the agents build via `memory.write`/`memory.recall`, CPE-524). CPE-954/955 made the Mailbox
lively, but nothing exercises Shared Memory, so that column stays "no notes yet…". Add demos that use the
memory tools heavily, add more messaging demos, and — mirroring CPE-955 — seed an initial memory note so the
column is visible even before agents post.

## Acceptance Criteria
- [x] New reusable `MEM` (heavy) + `MEM_LITE` instructions drive agents to `memory.write` linked notes
      (`[[Wiki Links]]` + tags) and `memory.recall` teammates' notes before adding.
- [x] Three Shared-Memory demos in a new "Shared memory · watch the Memory" dropdown group
      (`knowledge` / `research` / `glossarymem`).
- [x] Two more messaging demos (`standup`, `relay`) added to the "Messaging · watch the Mailbox" group.
- [x] Complex demos (`inventory`/`tour`/`testplan`) also fold in `MEM_LITE` so both panels populate.
- [x] Mission start seeds a coordinator "mission brief" note into shared memory (`seed_memory`, called from
      `handle_swarm_run`); browser-verified the panel shows Shared Memory (1) with the brief note.
- [x] `cargo test` (+1 `seed_memory` test) / clippy clean for `ai-console`; demos browser-verified — the
      dropdown shows 4 groups, the `knowledge` demo fills with `memory.write`/`memory.recall` instructions.

## Notes
Memory tool contract (`swarm_mcp.rs`): `memory.write { body, tags?, id? }` (body may contain `[[links]]`),
`memory.recall { query, tags?, n? }`, `memory.read { id }`. `/api/swarm/activity` parses `<mission>/memory/`
via `agent_memory::parse_note`; `renderSwarmActivity` shows each note's body + tags. Seed via
`FileStore::dispatch("coordinator","memory.write",…)` (the tested persist path). Pairs with [[CPE-955]].
