---
id: CPE-378
title: "Catalog auto-update toggle + pin/rollback (CPE-308 part 2, slice 4)"
type: Feature
status: Open
priority: Low
component: Multiple
tags: [big-design]
created: 2026-07-14
---

## Summary

The advanced catalog controls beyond manual refresh (CPE-374): a persisted **auto-update** toggle
(opt-in, default off) and per-agent **pin/rollback**. These need new backend state and an
anti-rollback override, so they're their own slice.

## Acceptance Criteria

- [ ] Auto-update toggle persisted (a field in the sidecar's stored state) + honoured (refresh on
      open when on). Default off, matching the CPE-296 no-unconsented-execution posture.
- [ ] Pin an agent to its current version (ignore newer) — persisted.
- [ ] Rollback to a prior version explicitly — the only sanctioned way past the CPE-372 anti-rollback
      (needs a host-side override path + a way to obtain the prior bundle).
- [ ] Provenance/version display per agent. Panel gets the standard visible border; visual QA noted.

## Notes — why `big-design`
Rollback specifically fights the anti-rollback invariant on purpose, so it needs a deliberate,
audited override (and a source of the older signed bundle). Persisted toggle/pin need new state +
endpoints. Depends on [[CPE-374]]/[[CPE-376]]. Part of [[CPE-308]].

## Work Log
2026-07-14 — Filed: split the state-dependent controls out of CPE-374.
