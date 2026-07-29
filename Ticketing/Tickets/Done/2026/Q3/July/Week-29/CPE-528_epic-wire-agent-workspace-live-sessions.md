---
id: CPE-528
title: "EPIC: Wire the Agent Workspace to live sessions (the integration layer)"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-16
closed: 2026-07-17
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
- [[CPE-541]] — Live session driver + live MCP servers *(Deferred; pure seams landed —
  `swarm_driver::apply_outcome` reporting reducer + `swarm_mcp` tool manifest/router, unit-tested — the
  live tail (real spawn, live stdio MCP host, real usage feed) + GUI QA remain, needing a running-app session)*

## Resolution (Done — 2026-07-17)
A **real multi-agent swarm now runs end-to-end from the AI Console**, verified live: an agent launches,
completes, appears as a tab, coordinates over the live mailbox + shared-memory MCP host, and the whole
thing is watchable on screen. Getting there took a chain of fixes, each found by tracing the actual
launch path (all Done, under this epic):

- **CPE-540** — assignment → launch-spec bridge (pure seam).
- **CPE-541** — live session driver + live MCP host (JSON-RPC/stdio, file-backed shared store) + real
  usage feed; verified with a real-process end-to-end test.
- **CPE-574** — swarm sessions adopted into the console (visible in Agent Watch) + a Windows-ConPTY
  completion fix.
- **CPE-583** — print-mode launch so agents complete and the driver advances.
- **CPE-585** — the form staffs one builder per task line.
- **CPE-586** — the launcher tab strip surfaces server-created swarm sessions.
- **CPE-587 → CPE-588** — task delivery: cmd-safe, then **verbatim** via stdin-redirect (Windows) / argv
  (Unix).
- **CPE-589** — native provider normalizes an OpenRouter-format model to a native alias.
- **CPE-590 / CPE-591** — variadic-safe fallback + field-wise catalog merge (stale download can't drop
  the `swarm` recipe).
- **CPE-582** — the real-agent GUI smoke: **PASSED** (mailbox post + memory note confirmed).
- **CPE-592** — live Swarm Coordination panel (mailbox + memory feed).

**Definition of Done met:** one click runs a swarm; agents coordinate over a live MCP host; it's
observable (tabs + Agent Watch + the coordination panel) and controllable. The pure cores from the five
program epics ([[CPE-501]]–[[CPE-505]]) are now a running product. Remaining Agent-Workspace polish
(broad Workbench browser origins CPE-527, the CPE-508 16-PTY smoothness measurement) is out of this
epic's swarm-integration scope and can be tracked separately if pursued.

## Status
**Done.** All 13 children delivered; the live swarm is verified in the running app.

## Notes
From [[CPE-500]]; consumes the shipped cores from all five program epics. `big-design` — this is where
the Agent Workspace stops being a set of tested libraries and becomes a running product. Informed by the
Herdr spike ([[CPE-511]]) — its socket control-API + agent-state ideas map directly onto this wiring.
