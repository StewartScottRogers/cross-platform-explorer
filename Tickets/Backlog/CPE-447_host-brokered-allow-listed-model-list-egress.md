---
id: CPE-447
title: "Host-brokered allow-listed model-list egress"
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
The host fetches a reseller's model list on the sidecar's behalf — allow-listed, no SSRF — extending the CPE-347/keyverify + CPE-433 pattern. Sidecar sends `{reseller}`, never a URL. Offline/proxy-aware (CPE-369).

## Acceptance Criteria
- [ ] Host maps `reseller` -> models endpoint from the manifest allow-list; refuses any host not on it; the sidecar never supplies a URL.
- [ ] Token attached from the per-reseller secret (CPE-452); never logged (Redactor).
- [ ] Offline -> no call (clear state, not an error); proxy/NO_PROXY honoured.
- [ ] Unit tests on the allow-list + URL builder (reuse forge_egress-style tests); threat-model row added (CPE-453).

## Notes
Feature-gated like `src-tauri/src/keyverify.rs` / `forge_egress.rs`. Reuse resolve_proxy/is_offline.
