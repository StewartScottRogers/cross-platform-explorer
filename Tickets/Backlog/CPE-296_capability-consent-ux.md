---
id: CPE-296
title: Capability consent & permission UX
type: Feature
status: Open
priority: High
component: Multiple
estimate: 2-3h
created: 2026-07-13
---

## Summary

"No ambient authority" needs a human in the loop. When a sidecar first requests
capabilities (context, secrets, storage, events, network), the user sees a clear
consent prompt — what it wants and why — and grants or denies per capability. The
broker ([[CPE-266]]) enforces the decision; grants are revocable later.

## Acceptance Criteria

- [ ] First-run consent sheet per sidecar listing each requested capability with a
      plain-language description and risk note (esp. secrets).
- [ ] Per-capability grant/deny; denial degrades the sidecar gracefully, never
      crashes it.
- [ ] Grants persisted, viewable and **revocable** in the management UI ([[CPE-274]]).
- [ ] A new capability requested after an update re-prompts for just that one.
- [ ] Tests: deny secrets → sidecar told, no secret access possible.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-266]]. **Phase:** P3. **Epic:** [[CPE-260]]. Pairs with
[[CPE-295]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
