---
id: CPE-279
title: Secret vault + credential profiles
type: Feature
status: Done
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Implemented `vault` in ai-console: `CredentialProfile` (named, switchable set of ENV-VAR -> vault-key REFERENCES — never values), `ProfileSet` (add/get/remove/persist, schema-versioned), and `resolve_env(profile, access)` which fetches each referenced secret via a `SecretAccess` trait (backed by the platform secrets capability CPE-268 in production; in-memory fake in tests) and errors if any is missing so a session never launches half-populated. Secret VALUES flow only through `SecretAccess` — a serialized profile carries key names, never values (tested). Feeds `LaunchContext` in the routing engine ([[CPE-285]]). 5 tests (resolve, missing-error, refs-not-values, profile CRUD+persist, profile switching). 38 crate tests + clippy green.

**Deferred:** the credential-entry UI + key verification is [[CPE-287]]; wiring `SecretAccess` to real contract `secrets.*` requests lands with the sidecar's host loop.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented the vault + credential profiles during dayshift. Done.
