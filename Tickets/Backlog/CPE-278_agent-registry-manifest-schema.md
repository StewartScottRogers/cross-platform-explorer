---
id: CPE-278
title: Agent registry + agent manifest schema
type: Task
status: Open
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
---

## Summary

The heart of "CLI-agnostic and extensible." A declarative agent manifest describes
each coding-agent CLI: id/name, detection command, per-OS install/update/uninstall
methods, run command + args, and the provider recipes it supports. The registry
loads bundled + user manifests so adding an agent is **data, not code**. Ported
from the per-agent script folders in `AgenticCliOptions`.

## Acceptance Criteria

- [ ] `agent.json` schema covering: detect, install/update/uninstall (per-OS +
      package-manager), run, supported providers, default model, plugin support.
- [ ] Registry loads bundled + user-dir manifests; user can add/override agents.
- [ ] Invalid manifests skipped with a logged reason, never fatal.
- [ ] Schema documented for the extensibility guide ([[CPE-293]]).
- [ ] Tests: parse, per-OS resolution, provider-recipe lookup.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-277]]. **Phase:** C1. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
