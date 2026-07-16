---
id: CPE-502
title: "EPIC: Swarm orchestration — role-based agent teams (ownership, mailbox, gates)"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-16
---

## Summary
Part of the **Agent Workspace** program (sibling to the AI Console [[CPE-261]]; from spike [[CPE-500]]).
Turn one prompt into a **coordinated team of role-based agents** — the BridgeSwarm analogue. The hardest
and most differentiating piece. A brief only until activated.

## Goal
One mission → a Coordinator dispatches Builders / Scout / Reviewer agents that divide the work, **never
collide on files**, message each other, and pass **quality gates** before a task is "done".

## Rough scope (NOT decomposed)
- Role model (coordinator / builder / scout / reviewer) — how a role is defined + assigned.
- **File-ownership locks**: each task exclusively owns the files it touches; shared dependencies
  sequenced automatically so concurrent agents never clobber each other.
- **Inter-agent mailbox** for coordination messages.
- **Quality gates** (tests/review) enforced before a task closes.

## Open questions (resolve at activation)
- How are roles/teams defined — a manifest (like the agent registry CPE-278) or ad-hoc?
- File-lock granularity (file vs dir vs path-glob) + how an agent acquires/releases ownership.
- Mailbox transport: reuse the MCP layer (CPE-288/307) or an internal bus?
- Cost/token controls for N concurrent agents; failure/retry + coordinator authority.

## Notes
From [[CPE-500]]; builds on the AI Console session engine + agent registry + MCP. `big-design`.
