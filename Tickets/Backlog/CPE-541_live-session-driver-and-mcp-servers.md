---
id: CPE-541
title: "Live session driver + live MCP servers (mailbox / memory) — needs the running app + GUI QA"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [needs-decision, big-design]
epic: CPE-528
estimate: 4h+
created: 2026-07-16
---

## Summary
The live core of [[CPE-528]]: actually **run** a swarm. Turn each coordinator `SwarmLaunch` ([[CPE-540]])
into a real launched Agent-Grid session (reuse the CPE-309 engine + the CPE-522 task hand-off), report
`on_done`/`on_failed`/`on_usage` back into the coordinator, and stand up **live MCP servers** exposing
the mailbox ([[CPE-516]]) + memory ([[CPE-525]]) tools so external agent processes coordinate + share
context.

## Why this can't be done headlessly (honest scope)
This is **live cross-process behavior** — spawning real agent processes, a live MCP server, and feeding
results back. Its correctness is only observable in the **running app**, not in unit tests; building it
"green" headlessly would be a facade. It needs interactive activation (architecture + security
decisions) and hands-on **GUI QA**.

## Acceptance Criteria
- [ ] A driver launches a real session per `SwarmLaunch` and reports results back to the coordinator.
- [ ] Live MCP server(s) expose mailbox + memory tools that external agents actually call.
- [ ] Budget/gate signals fed from real provider usage.
- [ ] Verified end-to-end in the running app (GUI QA), not just unit tests.

## Open questions (resolve when worked, with the user)
- One MCP server for mailbox+memory or two? How do agent processes discover it?
- Coordinator↔session bridge in the host vs the frontend? Failure/observability model?
- Security of the MCP exposure surface.

## Notes
The pure orchestration cores (coordinator, mailbox, memory, gates, budgets) + the [[CPE-540]] launch
bridge are done + unit-tested; this ticket is the live wiring + QA on top.
