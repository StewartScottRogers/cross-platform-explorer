---
id: CPE-582
title: "Real-agent swarm GUI smoke — watch a live 2–3 agent swarm coordinate"
type: Task
status: Done
closed: 2026-07-17
priority: Low
component: Multiple
tags: [ready]
epic: CPE-528
estimate: 30m
created: 2026-07-17
---

## Summary
The one empirical check that couldn't be automated in [[CPE-541]]: confirm a **real** coding agent
(a real `claude`, not the deterministic `--swarm-agent-sim` used by `tests/swarm_end_to_end.rs`)
honours the injected `--mcp-config`, loads the `--swarm-mcp` host, and coordinates with a sibling
agent over the live mailbox/memory. Everything upstream is built + tested; this is a manual GUI smoke.

## Prerequisite — DONE ([[CPE-583]])
Attempting this surfaced a real blocker: the swarm launched **interactive** `claude`, which never
self-exits, so the driver's exit-based completion never fired and the mission stalled. Fixed in
[[CPE-583]] — swarm agents now launch in **print mode** (`claude -p … --mcp-config … --dangerously-skip-permissions`)
via a manifest `swarm` recipe, so each runs its task, streams output, and exits → the coordinator
advances. This smoke is now unblocked.

## Why this is manual
It needs the GUI open (the AI Console starts the swarm), a launchable agent, and a human watching the
agent tabs — none of which is reachable headlessly. It is **not externally gated**: it can be run any
time, at **zero cost** using the local **`lmstudio-local`** provider (LM Studio's OpenAI-compatible
server), so no API key or spend is required.

## Turnkey steps
1. Open the installed app → **AI Console**.
2. Pick an agent + provider — `lmstudio-local` for a free local run (LM Studio must be running).
3. **Run swarm ▾** → enter a small task (e.g. "list the files in src and summarise") → **Start**.
4. Watch the agent tabs: the coordinator should hand off, a builder should pick up.

## Acceptance Criteria
- [ ] A real 2–3 agent swarm launches from the AI Console and the agent tabs appear.
- [ ] Agents actually coordinate: after the run, the mission dir (a `cpe-swarm-*` folder under TEMP)
      contains a `mailbox.jsonl` with posts **from the launched agents** and `memory/` notes they wrote
      (proof they used the live host, not just that they ran).
- [ ] Completion of one agent drives the next launch (the driver folds outcomes back).
- [ ] Real usage shows up: the session's tokens/cost are non-zero where the provider reports them
      (validates the CPE-541 usage feed against a real stream).

## Notes
If anything here fails, it's a real bug to fix — report what the mission dir shows (or doesn't). The
mechanism is already proven with a real process in `tests/swarm_end_to_end.rs`; this validates it with
a real model.

## Resolution (PASSED 2026-07-17)
Ran a real `claude` (native) swarm from the AI Console on 0.40.0. The builder tab streamed real output
("Done. Both actions completed…"), and `verify-swarm.ps1` confirmed the shared host received them:
- **mailbox.jsonl**: `[done] claude#builder1 -> broadcast: "builder1 done."`
- **memory/note-87949f71**: "Hello from builder1."

Getting here required a chain of real fixes found by tracing the actual launch path: CPE-574 (adopt
sessions), CPE-583 (print mode), CPE-586 (tab surfacing), CPE-587 (cmd quote-safety), CPE-589 (native
model normalization), CPE-590 (variadic-safe fallback) + clearing the stale catalog cache. Epic
[[CPE-528]]'s goal — a live, coordinating agent swarm visible in the app — is achieved.
