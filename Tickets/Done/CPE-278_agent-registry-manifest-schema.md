---
id: CPE-278
title: Agent registry + agent manifest schema
type: Task
status: Done
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Implemented `agents` in the `ai-console` crate: `AgentManifest` (schema_version, id,
name, per-OS `detect`/`install`/`update`/`uninstall`/`run` as `OsCommand`s, `providers`
list, `default_model`) with `*_for_current_os()` resolvers and `supports_provider`, plus
`AgentRegistry::load_from_dirs` scanning bundled + user dirs (user overrides bundled by
id), skipping malformed / no-run / unknown-future-schema manifests with recorded
`LoadWarning`s. Modelled on the `AgenticCliOptions` per-agent script folders, turned into
data. 5 tests (parse+resolve a real Claude manifest, skip malformed, skip no-run, skip
future schema, user override). 11 crate tests + clippy green.

**Deferred:** a `plugins`/MCP-support field on the manifest rides with the plugin system
([[CPE-288]]); the full authoring guide is [[CPE-293]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented the agent manifest schema + registry during dayshift. Done.
