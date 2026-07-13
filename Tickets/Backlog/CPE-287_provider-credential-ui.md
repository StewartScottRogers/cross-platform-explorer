---
id: CPE-287
title: Provider credential UI + key verification
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-13
---

## Summary

The UI to manage credential profiles and provider keys, backed by the vault
([[CPE-279]]). Port `SetOpenRouterKey.cmd` behavior — but securely: enter a key, it
is **verified** against the provider (e.g. OpenRouter `/api/v1/key`) and stored in
the OS keychain, never shown after entry, never persisted in plaintext.

## Acceptance Criteria

- [ ] Add/edit/remove provider keys and credential profiles from the sidecar UI.
- [ ] Live verification against the provider before saving; clear valid/invalid
      feedback.
- [ ] Key input masked; value never re-displayed or logged after save.
- [ ] One key shared across every agent that uses that provider (as in the
      reference).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-279]], [[CPE-285]]. **Phase:** C4. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
