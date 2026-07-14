---
id: CPE-383
title: "Catalog rollback to a specific prior version (enumeration + downgrade override)"
type: Feature
status: Open
priority: Low
component: Multiple
tags: [big-design]
created: 2026-07-14
---

## Summary

Beyond reset-to-shipped (CPE-379): roll an agent back to a specific *previously-published* catalog
version.

## Acceptance Criteria

- [ ] Enumerate prior published versions (GitHub Releases API — new allow-listed egress).
- [ ] Fetch a specific older signed bundle (`releases/download/<tag>/…`, not `latest`).
- [ ] Apply with a deliberate, audited **downgrade override** for the chosen agent(s) only
      (an `allow_downgrade` path in `apply_bundle`, not a blanket flag).
- [ ] UI: version picker + per-agent provenance/version display.

## Notes — why `big-design`
Deliberately defeats the CPE-372 anti-rollback invariant, so it needs a careful override + a trusted
source of the older bundle + release enumeration. Depends on [[CPE-379]]/[[CPE-376]]. Part of [[CPE-308]].

## Work Log
2026-07-14 — Split from CPE-379 (which delivered reset-to-shipped).
