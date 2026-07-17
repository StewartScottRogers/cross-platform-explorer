---
id: CPE-528
title: "EPIC: Wire the Agent Workspace to live sessions (the integration layer)"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-16
---

## Summary
The capstone of the **Agent Workspace** program ([[CPE-500]]). The five program epics — Agent Grid
([[CPE-501]]), Swarm ([[CPE-502]]), Board ([[CPE-503]]), Shared memory ([[CPE-504]]), Workbench
([[CPE-505]]) — shipped their logic as **pure, unit-tested cores**, deliberately leaving the parts that
can't be verified headlessly (launching real agent processes, live MCP-server exposure) as flagged
follow-ons. This epic is that **integration layer**: turn the cores into a *running* multi-agent
workspace. A brief only until activated.

## Goal
One click actually runs a swarm: the coordinator launches real Agent-Grid sessions per dispatch intent,
agents coordinate over a live mailbox + shared-memory MCP server, and the whole thing is observable and
controllable end-to-end — not just proven in isolation.

## Rough scope (NOT decomposed — the recorded gaps from each epic)
- **Swarm live driver** ([[CPE-517]]): turn each coordinator `Assignment` dispatch intent into a real
  launched Agent-Grid session (reuse the CPE-309 session engine + the CPE-522 task hand-off); report
  `on_done`/`on_failed`/`on_usage` back from the live session into the coordinator state machine.
- **Live mailbox over MCP** ([[CPE-516]]): register the `mailbox.post`/`read` tools as a real MCP server
  so external agent *processes* coordinate (not just in-process users).
- **Live shared-memory MCP server** ([[CPE-525]]): register `memory.write`/`read`/`recall` against the
  per-project `.agentmemory/` store so agents actually share context; persist writes.
- **Quality-gate + budget wiring** ([[CPE-518]]/[[CPE-519]]): run real gates (tests/review) and feed
  real provider usage into the budget/authority controls.
- **Workbench browser origins** ([[CPE-527]]): admit broad external https origins for the embedded
  browser under the Tauri capability model (localhost already works).
- **Runtime/GUI QA**: the end-to-end verification the pure cores couldn't cover (incl. the CPE-508
  16-PTY smoothness measurement).

## Open questions (resolve at activation)
- One live MCP server hosting both mailbox + memory tools, or two? How do agent processes discover it?
- How much of the coordinator↔session bridge lives in the AI Console host vs. the frontend?
- Failure/observability model: how are live-session crashes surfaced back into the coordinator + UI?
- Security: the mailbox/memory MCP server's exposure surface + the browser-origin admission (SSRF-ish).

## Decisions (activation 2026-07-16, on 'do them')
- **Split by verifiability:** the **unit-testable seam** (assignment → launch spec) is built now
  ([[CPE-540]]); the **live drivers** (spawning real sessions, live MCP servers, feeding results back)
  are [[CPE-541]] and require the **running app + GUI QA** — not fabricated headlessly.

## Child tickets (created at activation)
- [[CPE-540]] — Swarm → session launch-spec bridge (pure seam) *(Done, SPR-09)*
- [[CPE-541]] — Live session driver + live MCP servers *(Backlog; needs the running app + GUI QA)*

## Status
**In Progress.** Wave 1 (the pure bridge) is Done; the epic stays open because its essence — live
cross-process agent wiring — can only be completed + verified in the running app, which a headless build
can't do responsibly. It's ready for an interactive/QA session.

## Notes
From [[CPE-500]]; consumes the shipped cores from all five program epics. `big-design` — this is where
the Agent Workspace stops being a set of tested libraries and becomes a running product. Informed by the
Herdr spike ([[CPE-511]]) — its socket control-API + agent-state ideas map directly onto this wiring.
