---
id: CPE-279
title: Secret vault + credential profiles
type: Feature
status: Open
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
---

## Summary

The AI Console's secure credential layer, built on the platform secrets broker
([[CPE-268]]). Stores provider keys (OpenRouter, Anthropic, OpenAI, …) and
**environment login profiles** (switchable named credential sets for different
envs) in the OS keychain. Values are injected only into the spawned CLI's process
environment at launch — never written to disk in the clear, never logged, never
shown in the webview. Replaces the reference's `setx` + on-screen key handling.

## Acceptance Criteria

- [ ] Named credential profiles (provider keys + arbitrary env logins) stored via
      the secrets broker; switchable per session.
- [ ] Secrets injected into child-process env at spawn only; redacted everywhere
      else (logs, telemetry, UI).
- [ ] No plaintext persistence; fail closed if no OS store is available.
- [ ] Tests: profile CRUD, redaction, injection isolation.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-268]], [[CPE-277]]. **Phase:** C1. **Epic:** [[CPE-261]].
Directly addresses the "secure multi-env logins" requirement.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
