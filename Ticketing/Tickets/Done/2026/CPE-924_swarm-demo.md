---
id: CPE-924
title: Agent Deck — one-click "How-To-Demo" to exercise swarms
type: feature
component: Sidecar
priority: medium
tags: ready
created: 2026-07-23
status: Done
---

## Summary
Let a user try a **swarm** without knowing the syntax. Add a **"Try a demo"** button to the Agent Deck's
swarm controls that reveals the swarm task field **pre-filled** with two small, safe example tasks (each
scoped to its own file so they run in parallel) and shows a short **how-to** note explaining what will
happen. The user presses **Start** to launch it with their selected agent (the `native` provider needs no
API key). Also mention the demo in the swarm help section.

Framed as the app's first "How-To-Demo" — a guided, pre-filled example. A general demo framework across the
app is a possible future epic; this ships the concrete Agent Deck swarm demo.

## Acceptance Criteria
- [x] "Try a demo" button in the swarm controls reveals + pre-fills the task field with example tasks.
- [x] A how-to message explains what the demo will do; Start runs the real swarm path.
- [x] Covered by the launcher jsdom harness.

## Work Log
- 2026-07-23 — Filed + started.

- 2026-07-23 — Added "Try a demo" button + demoSwarm() to the Agent Deck launcher (reveals + pre-fills the swarm form with two safe disjoint example tasks + a how-to note; Start runs the real path). Extended swarm help + 09-swarms.md. jsdom harness: 63 tests pass (1 new).
