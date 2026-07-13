---
id: CPE-312
title: AI Console first-run onboarding
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-13
---

## Summary

The console has many moving parts (agents, providers, models, keys, profiles). A
first-run flow that gets a user from zero to a working session — detect/install a
first agent, add a provider key securely, launch — turns a powerful-but-daunting
tool into an approachable one.

## Acceptance Criteria

- [ ] Guided first-run: pick + install an agent, add/verify a provider key
      ([[CPE-287]]), launch a session ([[CPE-289]]).
- [ ] Skippable; never blocks power users.
- [ ] Explains the security model in plain language (where keys live, what consent
      means).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-289]], [[CPE-287]]. **Phase:** C6. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
