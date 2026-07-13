---
id: CPE-288
title: Plugin / extension system (MCP fan-out)
type: Feature
status: Open
priority: Medium
component: Backend
estimate: 4h+
created: 2026-07-13
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

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
