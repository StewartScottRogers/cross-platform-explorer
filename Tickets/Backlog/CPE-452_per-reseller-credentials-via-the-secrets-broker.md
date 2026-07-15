---
id: CPE-452
title: "Per-reseller credentials via the secrets broker"
type: Feature
status: Open
priority: High
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-15
epic: CPE-444
---

## Summary
Each reseller's API key is stored/resolved via the secrets broker in its own namespace (reuse CPE-344/348), injected for the model-list fetch + inference, never on disk/logs. Optional live key-check.

## Acceptance Criteria
- [ ] Per-reseller keychain namespace (`models/<reseller>/<label>`); labelled credentials like CPE-348.
- [ ] Key resolved for egress (CPE-447) at call time; never written to config or logged (Redactor).
- [ ] Key entry in the Keys panel gains a reseller selector; optional live verify against the reseller (allow-listed).
- [ ] Unit tests on the credential model + redaction; UI verify path noted as GUI.

## Notes
Reuse the AI Console vault/secrets broker (CPE-344) + labelled-credentials (CPE-348).
