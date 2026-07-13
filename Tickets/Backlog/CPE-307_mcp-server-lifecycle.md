---
id: CPE-307
title: MCP server lifecycle & credentials
type: Feature
status: Open
priority: Medium
component: Backend
estimate: 3-4h
created: 2026-07-13
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

## Work Log
2026-07-13 — Filed during epic-plan hardening.
