---
id: CPE-955
title: Seed the swarm Mailbox at mission start so coordination is always visible
type: bug
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-23
closed: 2026-07-23
epic: CPE-711
---

## Summary
The Agent Deck "Swarm coordination → Mailbox" panel showed "no messages yet…" on every run because
`init_mission` only writes the roster (`members.json`) — **nothing is ever seeded into `mailbox.jsonl`**.
So the feed depends entirely on an agent *choosing* to call `mailbox.post` over MCP; a native/demo agent
that never issues that tool call leaves the Mailbox empty, so "you don't always see messaging." CPE-954's
demo instructions help a cooperative agent, but the real fix is to guarantee visible coordination.

Fix: at mission start, seed the shared mailbox with the coordinator's opening posts — a **kickoff**
broadcast + one **assign** post per task — so the panel shows real messages the instant a swarm launches,
independent of agent behaviour. Agent `mailbox.post` chatter (incl. the teamchat demo) then appends to it.

## Acceptance Criteria
- [x] Mission start writes `mailbox.jsonl` with a kickoff broadcast + one assignment post per task
      (`seed_kickoff` in `swarm_mcp_server.rs`, called from `handle_swarm_run` after `init_mission`).
- [x] The coordination panel shows those messages immediately (no more "no messages yet…" on a fresh run) —
      browser-verified: feeding the seed records to `renderSwarmActivity` shows Mailbox (3) with a KICKOFF +
      two ASSIGN rows.
- [x] Seed format matches what `/api/swarm/activity` + `renderSwarmActivity` read (`{from,to,kind,body,ts}`);
      Rust test round-trips it through `FileStore::mailbox_records` (the same reader the endpoint uses).
- [x] `cargo test` (308 pass, +2 new) / clippy `--all-targets -D warnings` clean for `ai-console`;
      best-effort (append failure never breaks the mission); seed appends, agent posts follow.

## Notes
`seed_kickoff(dir, members, tasks)` in `swarm_mcp_server.rs`, called from `handle_swarm_run` in
`console.rs` right after `init_mission`. `Task` has `description` + `globs`. Sidecar backend change — the
next sidecar release bundles it (host rebuild). Pairs with [[CPE-954]] (the messaging demo).
