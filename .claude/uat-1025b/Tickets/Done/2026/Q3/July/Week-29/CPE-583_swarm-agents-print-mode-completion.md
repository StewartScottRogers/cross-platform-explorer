---
id: CPE-583
title: "Swarm agents run in print mode so the driver detects completion"
type: Bug
status: Done
priority: High
component: Sidecar
tags: [ready]
epic: CPE-528
estimate: 1-2h
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Found while attempting [[CPE-582]]: the `ProductionPlanner` composes an **interactive** launch
(`claude --model … --mcp-config <file> "<task>"` — no `-p`), and the `SwarmDriver` detects a task as
done **only** by the agent process exiting (`try_wait`/output-EOF). An interactive Claude Code TUI
never self-exits, so completion never fires → the coordinator never advances → the mission stalls to
the 600s timeout, which errors the run. The swarm would hang with real agents (GUI or headless).

## Fix (decided with the user)
Launch swarm agents in **print / non-interactive mode** so each runs its task to completion, streams
its output to its tab, and **exits** — letting the driver's `try_wait` fire and launch the next.
Implement it at the right altitude: a per-agent **`swarm` recipe** in the agent manifest (the
non-interactive invocation as *data*, with `{task}` + `{mcp_config}` placeholders), so the exact flags
are tunable during real QA without code changes — mirroring how `mcp_flag` was already "the one place
to adjust." The generic planner stays agent-agnostic.

## Acceptance Criteria
- [x] `AgentManifest` gains an optional `swarm` recipe (args template with `{task}`/`{mcp_config}`).
- [x] `ProductionPlanner` builds the launch from the agent's `swarm` recipe when present (substituting
      the task + per-agent MCP-config path); documented fallback when absent.
- [x] `claude.json` carries a `swarm` recipe using `-p` (print), the task, `--mcp-config`, and a
      non-interactive permission posture so tool calls don't block.
- [x] Tests: manifest deserializes the recipe; the planner composes a print-mode launch (`-p` + task +
      `--mcp-config` + config path); full sidecar suite + clippy green.

## Resolution
Made the swarm's agent invocation manifest-driven so agents run non-interactively and exit:
- `agents.rs` — `AgentManifest` gains `swarm: Option<SwarmRecipe>` (new `SwarmRecipe { args }`, an args
  template with `{task}` + `{mcp_config}` placeholders); `AgentManifest` now derives `Default`.
- `swarm_plan.rs` — `ProductionPlanner::swarm_args(agent, config, task)` builds `extra_args` from the
  agent's `swarm` recipe (placeholder substitution) when present, else the pre-existing positional
  `--mcp-config <file> <task>` fallback. Replaced the hardcoded `mcp_flag`.
- `agents/claude.json` — added a `swarm` recipe: `["-p", "{task}", "--mcp-config", "{mcp_config}",
  "--dangerously-skip-permissions"]`, so a swarm `claude` runs the task in print mode, calls its MCP
  tools without blocking on permission prompts, and exits — the driver's `try_wait` then fires and the
  coordinator advances.
- Tests: `swarm_recipe_templates_task_and_mcp_config`, `without_a_swarm_recipe_it_falls_back_to_the_
  positional_form`, and the existing claude-launch test now asserts `-p` + task-as-print-prompt +
  `--dangerously-skip-permissions`. Sidecar suite **282 passed / 0 failed**; `clippy --all-targets
  -D warnings` clean.

Unblocks [[CPE-582]]: agents now complete + exit, so a real mission advances. The exact Claude flags
(`-p`, permission posture, MCP tool-allow form) stay manifest **data**, tunable during the CPE-582 smoke
without a rebuild.

## Notes
Only `claude` has a `swarm` recipe so far (it's the agent that works via the native login). Other agents
fall back to the positional form and would need their own `swarm` recipe (their print-mode + MCP flags)
before they self-terminate in a mission — added as data when each is QA'd.
