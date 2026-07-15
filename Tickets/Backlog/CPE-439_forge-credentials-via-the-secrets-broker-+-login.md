---
id: CPE-439
title: "Forge credentials via the secrets broker + login"
type: Feature
status: Open
priority: High
component: Backend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
epic: CPE-429
---

## Summary
Per-provider auth (CPE-429): OAuth tokens / PATs / SSH keys in the OS keychain via the secrets broker
(CPE-268), with per-provider login flows. Never plaintext, never logged.

## Acceptance Criteria
- [ ] Store/retrieve a provider credential (PAT first; OAuth device-flow where supported).
- [ ] Injected into host-brokered API calls (CPE-433) + git ops (CPE-436) at use time only.
- [ ] Namespaced per provider; consent-gated.
