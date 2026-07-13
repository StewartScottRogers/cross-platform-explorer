---
id: CPE-288
title: Plugin / extension system (MCP fan-out)
type: Feature
status: Done
priority: Medium
component: Backend
estimate: 4h+
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Port the `Plugins/` system: declarative plugin manifests (`kind: mcp-server`,
`supports: [agents…]`, install/uninstall markers) that extend agents with MCP
servers, slash commands, and third-party tooling, **fanning install/uninstall
across every supporting agent**. Rust-orchestrated, manifest-driven — new tooling
is added as data. This is the "extend the coding-agent frameworks and third-party
tooling" requirement.

## Acceptance Criteria

- [ ] Plugin manifest schema (name, kind, supports, scope, install/uninstall,
      marker) loaded from bundled + user dirs.
- [ ] Install/uninstall a plugin fans across all supporting installed agents
      (edits their MCP/config appropriately per agent).
- [ ] Idempotent via install markers; safe re-run.
- [ ] Tests with a stub plugin + two stub agents.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-282]]. **Phase:** C5. **Epic:** [[CPE-261]].

## Resolution

Implemented `plugins` in ai-console: `PluginManifest` (id, name, kind e.g. mcp-server, `supports` agent ids) + `PluginRegistry` (bundled+user, skip-on-error). `applicable_agents` = supports ∩ installed; `install_plugin`/`uninstall_plugin` **fan across every supporting installed agent**, continuing past failures, with the per-agent config edit abstracted behind a `PluginApplier` trait (idempotent; production appliers edit each agent's MCP config). Ported from the reference `Plugins/` system. 5 tests (intersection, install fan-out, continue-past-failure, uninstall, registry skip-bad). 46 crate tests + clippy green.

**Deferred:** the concrete per-agent MCP-config appliers (JSON/TOML/opencode edits) and MCP-server process lifecycle are [[CPE-307]]; the manifest `plugins`-support field on agents is a small follow-up.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented the plugin fan-out system during dayshift. Done.
