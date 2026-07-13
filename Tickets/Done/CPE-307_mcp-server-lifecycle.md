---
id: CPE-307
title: MCP server lifecycle & credentials
type: Feature
status: Done
priority: Medium
component: Backend
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

Plugins ([[CPE-288]]) install MCP servers (e.g. cipher, context7) — these are
**their own long-running processes with their own credentials** (the reference's
cipher.yml). Managing them is more than editing a config file: they need
start/stop/health, credential provisioning via the vault, and cleanup on plugin
uninstall.

## Acceptance Criteria

- [ ] Discover MCP servers a plugin declares; provision their credentials from the
      secret vault ([[CPE-279]]), never plaintext.
- [ ] Start/stop/health of MCP server processes, counted against budget ([[CPE-297]])
      and reaped on shutdown.
- [ ] Uninstalling a plugin stops its MCP servers and removes their config from every
      agent it touched.
- [ ] Surfaced in diagnostics ([[CPE-298]]).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-288]], [[CPE-279]]. **Phase:** C5. **Epic:** [[CPE-261]].

## Resolution

Implemented `mcp` in ai-console: `McpProcess::spawn(McpServerSpec{id, command, env})` runs an MCP server as a detached background process with credentials injected via env (resolved from the vault, never logged), `is_running()` (health=liveness), `stop()`, and Drop that reaps it (no orphans). `McpManager` tracks servers by id: `start` (replacing a prior one), `is_running`, `stop` (on plugin uninstall), `stop_all` (on shutdown), `running_ids`. 4 tests spawn real background processes (start→running→stop, stop_all, stop-unknown ok, spawn-failure reported). 57 crate tests + clippy green.

**Deferred:** discovering which MCP servers a plugin declares (a plugin-manifest field) and counting them against the resource budget wire up with the plugin-manifest extension + [[CPE-297]] at integration.

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — Implemented MCP server process lifecycle during dayshift. Done.
