---
id: CPE-590
title: "Swarm 'MCP config not found: <task>' — variadic --mcp-config slurps the task in the no-recipe fallback"
type: Bug
status: Done
priority: High
component: Sidecar
tags: [ready]
epic: CPE-528
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Even on 0.39.0 (with the model + quote fixes), a swarm agent errored:
`Invalid MCP configuration: MCP config file not found: <cwd>\<the task text>`. `--mcp-config` received
the **task** as its value.

## Root cause
The running sidecar loads an **auto-downloaded catalog** (`AppData/Roaming/…/ai-console-catalog/
claude.json`, dated 2026-07-15) that **predates CPE-583 and has no `swarm` recipe** — it shadows the
bundled catalog. So `agent.swarm` is `None` and `ProductionPlanner::swarm_args` used the **fallback**
`["--mcp-config", <cfg>, <task>]`. Claude's `--mcp-config <configs...>` is **variadic/greedy**, so with
the task right after the config path it slurps *both* — treating the task as a second config file → "not
found". (The recipe path is fine; only the fallback was unsafe, and the stale catalog forces the fallback.)

## Fix
Make the fallback **variadic-safe**, mirroring the print-mode recipe:
`["-p", <task>, "--mcp-config", <cfg>, "--dangerously-skip-permissions"]` — the task goes via `-p`, and
`--mcp-config <cfg>` is terminated by a trailing flag so only the config is consumed. Verified this exact
arg order coordinates end-to-end through the real `cmd`/PTY path.

## Acceptance Criteria
- [x] Fallback never places the task after `--mcp-config`; a flag follows the config path.
- [x] Test asserts the variadic-safe order. swarm_plan 8 passed; clippy green.

## Follow-on
[[CPE-591]] — the auto-updated catalog shadows the (newer) bundled catalog, hiding recent manifest
changes (the swarm recipe, etc.). Bundled should win when newer, or the cache should be invalidated on
app update.
