---
id: CPE-439
title: "Forge credentials via the secrets broker + login"
type: Feature
status: Done
priority: High
component: Backend
tags: [ready]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-429
---

## Summary
Per-provider auth (CPE-429): OAuth tokens / PATs / SSH keys in the OS keychain via the secrets broker
(CPE-268), with per-provider login flows. Never plaintext, never logged.

## Acceptance Criteria
- [x] Store/retrieve a provider credential (PAT first; OAuth device-flow where supported). — **PAT done** via the OS keychain; OAuth device-flow is a noted follow-up.
- [x] Injected into host-brokered API calls (CPE-433) + git ops (CPE-436) at use time only.
- [x] Namespaced per provider; consent-gated (opt-in Remember).

## Resolution
Forge tokens now persist in the OS keychain so browse/clone don't need re-entry. New feature-gated host commands `forge_set_token`/`forge_get_token`/`forge_delete_token` reuse the host's `KeyringBackend` (Windows Credential Manager / macOS Keychain / Linux Secret Service) under a dedicated `com.cross-platform-explorer.forge` service, account = provider id — namespaced apart from sidecar secrets; the token is never logged. RepoBrowser loads a saved token on mount, offers a **Remember** checkbox (opt-in = consent), persists on a successful browse (proves the token works) and forgets on uncheck; the token is passed per-call into `forge_browse` (CPE-433) + `forge_clone` (CPE-436), never stored in the request. 7 RepoBrowser tests (incl. saved-token load); svelte-check 0, clippy clean both feature modes. **Follow-up:** OAuth device-flow login (PAT is the mechanism today); the keychain round-trip itself is GUI/runtime-verified (tests cover the wiring + frontend).
