---
id: CPE-308
title: Agent catalog update / subscription
type: Feature
status: Open
priority: Medium
component: Backend
tags: [big-design]
estimate: 3-4h
created: 2026-07-13
---

## Summary

"Keep up with the coding-agent market without ricochet" needs the catalog itself to
refresh without an app release. Let the bundled agent/provider/plugin manifests
update from a signed remote source (or a user-pointed one), so new agents and
changed install recipes arrive as data.

## Acceptance Criteria

- [ ] Fetch/refresh catalog manifests from a configured source; signature-verified
      ([[CPE-295]]) before trust.
- [ ] New/updated manifests slot into the registry ([[CPE-278]]) with no code change;
      schema-migrated if needed ([[CPE-300]]).
- [ ] User controls: manual refresh, auto-update toggle, pin/rollback a manifest
      version.
- [ ] Offline-safe: last-known-good catalog keeps working ([[CPE-310]]).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-295]]. **Phase:** C6. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
