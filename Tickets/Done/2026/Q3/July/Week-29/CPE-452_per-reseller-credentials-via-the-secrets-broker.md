---
id: CPE-452
title: "Per-reseller credentials via the secrets broker"
type: Feature
status: Done
priority: High
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-444
---

## Summary
Each reseller's API key is stored/resolved via the secrets broker in its own namespace (reuse CPE-344/348), injected for the model-list fetch + inference, never on disk/logs. Optional live key-check.

## Acceptance Criteria
- [x] Per-reseller keychain namespace (`models/<reseller>/<label>`); labelled credentials like CPE-348.
- [x] Key resolved for egress (CPE-447) at call time; never written to config or logged (Redactor).
- [x] Key entry in the Keys panel gains a reseller selector; optional live verify against the reseller (allow-listed).
- [x] Unit tests on the credential model + redaction; UI verify path noted as GUI.

## Notes
Reuse the AI Console vault/secrets broker (CPE-344) + labelled-credentials (CPE-348).

## Work Log
2026-07-15 — Backend done: `reseller_secret_name()` (keychain `reseller:<id>[#label]`, namespaced apart from provider keys), `POST /api/reseller-keys` (set) + `/delete`, and the picker's `/api/models` token now resolves from it (CPE-447 egress). Test proves set→resolve→delete round-trips in its own namespace (no collision with `provider:`). ai-console tests green, clippy clean. KEPT OPEN: the Keys-panel reseller selector UI + optional live verify are the GUI tail (need a real run).

## Resolution (UI completed)
The Keys panel now manages reseller keys end-to-end: the key dropdown lists the resellers (value `reseller:<id>`); Save/Remove route to `/api/reseller-keys` (vs `/api/keys` for providers); the list shows stored reseller keys (new `GET /api/reseller-keys` probes the known resellers for a stored key — names only, never values); Check shows "no live check for resellers" (there's no reseller verify endpoint). Backend namespace + resolve were done earlier; this completes the AC. Tests: console list round-trip (set→list→delete) + a launcher jsdom test (resellers in the dropdown; save routes to the reseller endpoint, never the provider path). ai-console clippy clean, launcher suite 21 green. Final feel is a GUI eyeball.
