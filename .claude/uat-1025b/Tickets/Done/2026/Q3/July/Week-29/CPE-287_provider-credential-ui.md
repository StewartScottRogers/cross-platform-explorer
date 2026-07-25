---
id: CPE-287
title: Provider credential UI + key verification
type: Feature
status: Done
closed: 2026-07-13
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

## Work Log
2026-07-13 (Dayshift) — Delivered as three sub-tickets after the user chose to proceed:
- **CPE-344** — sidecar↔host secrets broker client (`BrokerClient`/`BrokerSecrets` over the
  stdio envelope channel → OS keychain) + inject the provider's stored key at launch (one key
  shared across every agent using that provider).
- **CPE-345** — key-management API: store (format-checked) / list (names only, values never
  returned) / remove / offline verify.
- **CPE-346** — launcher "Keys…" UI: masked input, check, save (input cleared — value never
  re-displayed), list with Remove.

Acceptance status: add/remove provider keys ✅; masked & never re-displayed ✅; one key shared
per provider ✅. **Live provider verification** deferred to **CPE-347** (needs the Network
capability — ai-console has no TLS client and shouldn't call out directly). **Named credential
profiles UI** (the `ProfileSet` half of "profiles") deferred to **CPE-348**. Closing the
umbrella: the core secure-key workflow is complete and on `main`; the two follow-ups are
scoped and filed. Note: launcher UI needs a visual eyeball; the backend is unit-tested.
