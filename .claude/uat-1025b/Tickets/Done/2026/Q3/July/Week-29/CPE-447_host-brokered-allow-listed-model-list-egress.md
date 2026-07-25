---
id: CPE-447
title: "Host-brokered allow-listed model-list egress"
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
The host fetches a reseller's model list on the sidecar's behalf — allow-listed, no SSRF — extending the CPE-347/keyverify + CPE-433 pattern. Sidecar sends `{reseller}`, never a URL. Offline/proxy-aware (CPE-369).

## Acceptance Criteria
- [x] Host maps `reseller` -> models endpoint from the manifest allow-list; refuses any host not on it; the sidecar never supplies a URL.
- [x] Token attached from the per-reseller secret (CPE-452); never logged (Redactor).
- [x] Offline -> no call (clear state, not an error); proxy/NO_PROXY honoured.
- [x] Unit tests on the allow-list + URL builder (reuse forge_egress-style tests); threat-model row added (CPE-453).

## Notes
Feature-gated like `src-tauri/src/keyverify.rs` / `forge_egress.rs`. Reuse resolve_proxy/is_offline.

## Resolution
Added `src-tauri/src/models_egress.rs` (feature-gated like keyverify/forge_egress): `models_endpoint(reseller)` is the host-authoritative allow-list (openrouter/together/fireworks/groq/deepinfra/novita/aimlapi/wavespeed/github-models; github carries X-GitHub-Api-Version), so the sidecar sends only `{reseller}` — never a URL (no SSRF). `list_models()` (sidecar-platform) fetches with the token in the reseller's auth header, reusing keyverify resolve_proxy/is_offline, bounds the body (8 MiB), never logs the token. Wired live: `list_models_response` + a `host.list_models` arm in the AI Console request router (mirrors host.verify_key) — genuinely callable end-to-end. 3 unit tests on the allow-list; clippy clean both feature modes. Threat-model row added in CPE-453.
