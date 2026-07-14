---
id: CPE-311
title: Usage/cost tracking & opt-in telemetry
type: Feature
status: Open
priority: Low
component: Multiple
tags: [needs-decision]
estimate: 2-3h
created: 2026-07-13
---

## Summary

Running agents against paid providers costs money and tokens. Optionally surface
per-session usage/cost so the user isn't surprised. Any product telemetry is
strictly opt-in and privacy-preserving — never prompts, code, or secrets.

## Acceptance Criteria

- [ ] Where the provider exposes it, show per-session token/cost usage in the
      console; aggregate per agent/provider.
- [ ] Product telemetry (if any) is opt-in, documented, and contains no repo
      contents, prompts, or secret values.
- [ ] A clear off switch; off means no outbound telemetry at all.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-285]], [[CPE-292]]. **Phase:** C6. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
