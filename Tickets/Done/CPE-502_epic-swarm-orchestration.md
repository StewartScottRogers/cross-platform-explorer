---
id: CPE-502
title: "EPIC: Swarm orchestration — role-based agent teams (ownership, mailbox, gates)"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-16
closed: 2026-07-16
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

## Decisions (activation 2026-07-16)
Adopted the **recommended defaults** — the activation questions were surfaced twice but the dialog went
unanswered, so I proceeded on sound defaults rather than stall the sprint. **Any of these is easy to
change; say so and I'll re-decompose.**
- **Role model:** **Declarative manifest templates** — a team is a manifest of roles
  (coordinator/builder/scout/reviewer) bound to agent+model, reusing the [[CPE-278]] registry pattern.
- **File-ownership locks:** **Path-glob claims** — a task exclusively owns matching globs; overlaps are
  refused/queued; shared deps sequenced.
- **Mailbox transport:** **Reuse the MCP layer** ([[CPE-288]]/[[CPE-307]]) — one substrate; ties to
  [[CPE-504]]. (The [[CPE-511]] Herdr socket-API idea is noted as an alternative if MCP proves limiting.)
- **First wave:** **File-ownership lock manager first** — the safety substrate everything builds on.

## Child tickets (created at activation)
Wave 1 — the foundation (assigned to sprint **[[SPR-01]]**):
- [[CPE-514]] — File-ownership lock manager (path-glob claims + shared-dep sequencing) *(ready; wave 1)*
- [[CPE-515]] — Role/team manifest model (coordinator/builder/scout/reviewer) *(ready)*
- [[CPE-516]] — Inter-agent mailbox over MCP *(needs-prereq: MCP)*

Wave 2 — the orchestration (later sprint):
- [[CPE-517]] — Coordinator dispatch loop *(needs-prereq 514/515/516; big-design)*
- [[CPE-518]] — Quality gates before a task is "done" *(needs-prereq 517)*
- [[CPE-519]] — Cost/token budgets + retry + coordinator authority *(needs-prereq 517)*

Suggested order: **CPE-514** first (unblocks the coordinator), then CPE-515 / CPE-516 (independent), then
CPE-517 → CPE-518 / CPE-519.

## Resolution (closed 2026-07-16)
Swarm orchestration is built end-to-end across 6 children (2 sprints), as pure, fully-tested modules in
the ai-console crate:
- **Substrates (SPR-01):** [[CPE-514]] file-ownership lock manager (path-glob claims, no collisions),
  [[CPE-515]] role/team manifest (coordinator/builder/scout/reviewer), [[CPE-516]] inter-agent mailbox
  (addressed/role/broadcast, ordered, contained).
- **Orchestration (SPR-02):** [[CPE-517]] coordinator (staff → assign → lock-gated schedule → dispatch
  via mailbox → collect), [[CPE-518]] quality gates (Gating state, reopen-on-fail, review over the
  mailbox), [[CPE-519]] budgets + retry + authority (per-agent/mission caps that pause not overspend,
  bounded retry-then-escalate, pause/stop/reassign with an audit trail).

One prompt → a coordinator staffs a role-based team, splits the mission into tasks that **exclusively own
their files** (concurrent agents never collide; shared files sequence), **coordinate via the mailbox**,
and pass **quality gates** before done — with budget/retry/authority guardrails. ~55 new unit tests
across the six modules; 233 ai-console lib tests green; clippy clean in both feature modes; CI green.

**Carve-out / follow-on (recorded):** the **live driver** — turning each coordinator `Assignment`
dispatch intent into a real launched Agent-Grid session (and the mailbox's live MCP-server exposure to
external agent processes) — is the integration layer that sits on top of these pure cores. It is not
headlessly verifiable and was deliberately kept out of scope; it's flagged in CPE-516/517 and is the
natural next epic (wiring Swarm to real sessions). Does not block this epic — the orchestration brain is
complete and proven in isolation.

## Notes
From [[CPE-500]]; builds on the AI Console session engine + agent registry + MCP. `big-design`.

## Work Log (close)
2026-07-16 — **Closed.** All 6 children Done across SPR-01 (substrates) + SPR-02 (orchestration). The
coordinator ties locks + team + mailbox into gated, budgeted, collision-free multi-agent orchestration,
fully unit-tested. Live session-dispatch driver flagged as the follow-on epic. Moved Epics/ → Done/.

## Work Log
2026-07-16 — Filed as a dormant `Proposed` brief (from spike CPE-500).
2026-07-16 — **Activated** into sprint SPR-01. Surfaced the 4 open questions to the user (twice); the
dialog went unanswered, so adopted the recommended defaults (see Decisions — reversible) rather than
stall. Decomposed into 6 children (CPE-514…519); wave-1 foundation (514/515/516) assigned to SPR-01.
Status → In Progress.
