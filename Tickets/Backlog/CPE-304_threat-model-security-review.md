---
id: CPE-304
title: End-to-end threat model & security review (milestone)
type: Task
status: Open
priority: Critical
component: Multiple
estimate: 4h+
created: 2026-07-13
---

## Summary

A dedicated security milestone before the platform is declared production-ready.
This system spawns arbitrary CLIs, injects credentials, embeds remote-ish UI, and
loads user-supplied manifests — the attack surface is real. Produce a written
threat model and run a review covering the whole boundary.

## Acceptance Criteria

- [ ] STRIDE-style threat model across: IPC channel, capability broker, secrets
      broker, manifest trust, embedded UI/CSP, spawned agent & MCP processes.
- [ ] Each threat has a mitigation mapped to a ticket ([[CPE-268]], [[CPE-275]],
      [[CPE-295]], [[CPE-296]], [[CPE-306]], [[CPE-307]]) or a new one.
- [ ] Verifies: no plaintext secrets at rest, no secret in logs/UI, no
      cross-sidecar reach, no unconsented code execution, no UI escape to explorer.
- [ ] Sign-off recorded in the ADR ([[CPE-259]]); gaps filed as blockers.
- [ ] Repeatable checklist so each new tenant sidecar gets a lightweight review.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-275]], [[CPE-295]], [[CPE-296]], [[CPE-268]]. **Phase:** P5
(and re-run per tenant). **Epic:** [[CPE-260]]; applies to [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
