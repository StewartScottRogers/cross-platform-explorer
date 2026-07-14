---
id: CPE-347
title: "Live provider key verification (needs the Network capability)"
type: Feature
status: Open
priority: Low
component: Backend
created: 2026-07-13
---

## Summary

CPE-345 added an *offline* key format pre-check (`/api/keys/verify` returns `live:false`).
True live verification — calling the provider (e.g. OpenRouter `GET /api/v1/key`, OpenAI
`GET /v1/models`) and reporting valid/invalid before saving — is deferred because ai-console
has no TLS HTTP client and does not hold the `Network` capability. The sidecar should NOT make
arbitrary outbound calls directly; route it through the host.

## Scope
- Request `Capability::Network` in `REQUESTED_CAPABILITIES`; host provides a `network.fetch`
  provider (allow-listed hosts) or equivalent.
- ai-console: a `verify_key(provider, key)` that issues `network.fetch` via the broker client
  (CPE-344) to the provider's check endpoint; map 2xx→valid, 401/403→invalid.
- `/api/keys/verify` returns `{valid, live:true, detail}`; the launcher UI shows the live result.

## Acceptance
- Entering a wrong key is rejected with a live "invalid key" before it's stored; a good key
  verifies. Allow-list prevents the capability being a general SSRF primitive.
