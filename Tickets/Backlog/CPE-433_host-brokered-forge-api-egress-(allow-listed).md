---
id: CPE-433
title: "Host-brokered forge API egress (allow-listed)"
type: Feature
status: Open
priority: High
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-15
epic: CPE-429
---

## Summary
The repos sidecar has no network client; the host performs allow-listed API calls on its behalf (no
SSRF), extending threat-model section 7. The allow-list is the union of each provider manifest api_hosts.

## Acceptance Criteria
- [ ] host.forge_request {provider, method, path, body?}: host builds the URL from api_hosts (sidecar
      never supplies a full URL), attaches the stored token, returns the response; proxy/offline-aware.
- [ ] Only allow-listed hosts reachable; a path cannot escape the host; token never logged.
- [ ] Unit tests on the URL builder + allow-list; threat-model row added.
