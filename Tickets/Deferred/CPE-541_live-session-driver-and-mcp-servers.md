---
id: CPE-541
title: "Live session driver + live MCP servers (mailbox / memory) — needs the running app + GUI QA"
type: Feature
status: Deferred
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
Each AC has a **pure seam** (headlessly buildable + testable, done this pass) and a **live tail**
(spawning real processes / a live stdio server / GUI QA — only verifiable in the running app, deferred).

- [ ] A driver launches a real session per `SwarmLaunch` and reports results back to the coordinator.
  - [x] Reporting reducer: `swarm_driver::apply_outcome(coord, &SessionOutcome)` folds a finished
        session (usage-first, then done/failed) into the coordinator and returns the next assignments —
        4 unit tests.
  - [ ] Live tail: spawn the `SwarmLaunch` (via `scope::build_launch` → `SessionEngine::launch`), detect
        real completion, and call `apply_outcome`. *(running app)*
- [ ] Live MCP server(s) expose mailbox + memory tools that external agents actually call.
  - [x] Tool contract + router: `swarm_mcp::tools_manifest()` (6 tools) + `dispatch_tool(...)` routing
        `mailbox.*` / `memory.*` onto the in-process mailbox + memory — 7 unit tests.
  - [ ] Live tail: one in-process **stdio** MCP host serving that router, injected into each launched
        agent's MCP config. *(running app)*
- [ ] Budget/gate signals fed from real provider usage.
  - [x] Usage→budget wiring proven: `apply_outcome` reports usage *before* completion, so a real cap
        pauses the agent/mission before re-dispatch (unit-tested).
  - [ ] Live tail: feed `SessionOutcome.tokens/cost_millis` from the real provider-usage stream. *(running app)*
- [ ] Verified end-to-end in the running app (GUI QA), not just unit tests. *(deferred — see checklist)*

## Open questions (resolve when worked, with the user)
- One MCP server for mailbox+memory or two? How do agent processes discover it?
- Coordinator↔session bridge in the host vs the frontend? Failure/observability model?
- Security of the MCP exposure surface.

## Notes
The pure orchestration cores (coordinator, mailbox, memory, gates, budgets) + the [[CPE-540]] launch
bridge are done + unit-tested; this ticket is the live wiring + QA on top.

## Decisions (2026-07-16)
Put the three open questions to the user via AskUserQuestion; the user did not select, so proceeded on
the recommended defaults (the conservative, smallest-surface options — recorded here as the design of
record for the live tail):
- **MCP surface:** *one* in-process host exposing both `mailbox.*` + `memory.*` tools over **stdio** —
  no network listener, no port, no bearer token (smallest attack surface; the epic's SSRF-ish concern
  disappears). Discovery = the tools are injected into each launched agent's MCP config.
- **Bridge home:** the **AI Console host (Rust sidecar)** owns the driver, reusing the in-process
  `Coordinator` + `SessionEngine` + `Mailbox` + `MemoryGraph` directly (no IPC round-trips; callbacks
  fire in-process). The frontend only observes/controls.
- **Scope this pass:** build the headlessly-verifiable seams + unit-test them; **defer** the live spawn,
  live stdio server, and GUI QA (a "green" headless build of those would be a facade — the ticket's own
  warning).

## Resolution (partial — pure seams landed; live tail deferred)
Landed the two transport-free seams of the live wiring, both unit-tested, so the remaining work is
*only* the parts that genuinely need a running app:
- `sidecar/ai-console/src/swarm_driver.rs` — `SessionOutcome` + `apply_outcome(coord, &outcome)`: the
  reporting reducer that folds a finished session into the coordinator (usage-first so budget caps trip
  before re-dispatch, then `on_done`/`on_failed`) and returns the next assignments. 4 tests.
- `sidecar/ai-console/src/swarm_mcp.rs` — `tools_manifest()` (the MCP `tools/list` payload, 6 tools) +
  `dispatch_tool(mailbox, memory, from, tool, args, ts)` routing `mailbox.post/read/drain` +
  `memory.*` onto the in-process APIs. `from` is host-supplied (an agent can't spoof another's id). 7 tests.
- Registered both in `lib.rs` (`apply_outcome`/`SessionOutcome`, `swarm_tool_call`/`swarm_tools_manifest`).
- Verified: full sidecar lib suite **258 passed / 0 failed** (incl. 11 new); `cargo clippy --all-targets
  -D warnings` clean. (Sidecar crate has no feature flags, so one clippy mode covers it.)

**Why Deferred, not Done:** every AC has a live tail (spawn a real process / serve live stdio / feed real
provider usage / GUI QA) whose correctness is only observable in the running app. Nothing *external*
gates it — it just needs an interactive running-app session, so it stays pickable. Building those tails
"green" headlessly would be the facade the ticket explicitly warns against.

## Next Actions — live/QA session (turnkey)
1. **Live driver:** for each `SwarmLaunch` (from `launch_spec_for`), resolve the `AgentManifest` +
   provider, `scope::build_launch(...)` → `SessionEngine::launch(id, &PtyLaunch)`; on session EOF/exit
   build a `SessionOutcome` and call `apply_outcome`, then launch the returned assignments.
2. **Live MCP host:** wrap `swarm_mcp::dispatch_tool` in a stdio JSON-RPC MCP server (one per mission),
   inject its tool config into each launched agent so `from` = that agent's instance id.
3. **Usage feed:** map the real provider-usage stream (see `usage.rs`) into `SessionOutcome.tokens/cost_millis`.
4. **GUI QA:** run a real 2–3 agent swarm end-to-end; confirm agents coordinate over the live mailbox +
   share memory, budget caps pause live, and a crashed session reports `on_failed`. (Incl. the CPE-508
   16-PTY smoothness check from the epic.)

## Work Log
2026-07-16 — Picked up. Estimate: 4h+ (unchanged; live cross-process wiring + GUI QA). This is
`needs-decision` + `big-design` and can't be *verified* headlessly. Surveyed the seams — coordinator
callbacks (`on_done`/`on_failed`/`on_usage`), `SessionEngine::launch(id, &PtyLaunch)` (Local+Daemon),
`scope::build_launch` (`SwarmLaunch`→`PtyLaunch`), in-process `Mailbox` (post/read/drain) +
`agent_memory::memory_tool`, and `mcp.rs` (spawns external MCP *processes* only; no in-process host).
2026-07-16 — Put the 3 open architecture/security questions to the user; no selection made → proceeded on
the recommended defaults (one stdio MCP host; bridge in the sidecar; build pure seams + defer live tail).
2026-07-16 — Built `swarm_driver.rs` (reporting reducer, 4 tests) + `swarm_mcp.rs` (tool manifest + router,
7 tests); registered both in `lib.rs`. Full sidecar suite 258 passed / 0 failed; clippy `-D warnings` clean.
2026-07-16 — Moved to Deferred. Deferred-on: needs a running-app + GUI-QA session for the live tails (real
process spawn, live stdio MCP host, real usage feed) — not externally gated, so pickable anytime. Revisit-
when: next interactive/live session; the turnkey checklist above is the entry point. Epic [[CPE-528]] stays
In Progress.
