---
id: CPE-379
title: "Catalog explicit rollback to a prior version (CPE-308 part 2, slice 4b)"
type: Feature
status: Open
priority: Low
component: Multiple
tags: [big-design]
created: 2026-07-14
---

## Summary

Let the user roll an agent back to a previously-published catalog version — the only sanctioned way
past the CPE-372 anti-rollback invariant.

## Acceptance Criteria

- [ ] Enumerate prior published catalog versions (GitHub Releases list / per-release assets).
- [ ] Fetch a specific older signed bundle (`releases/download/<tag>/…`, not `latest`).
- [ ] Apply with a deliberate, audited **downgrade override** for the chosen agent(s) only.
- [ ] UI: pick a version to roll back to; provenance/version display per agent.

## Notes — why `big-design`
Deliberately defeats anti-rollback, so it needs a careful override path (not a general flag) + a way
to obtain + trust the older bundle (release enumeration via the GitHub API = new egress/allow-list).
Depends on [[CPE-378]]/[[CPE-376]]. Part of [[CPE-308]].

## Work Log
2026-07-14 — Filed: split from CPE-378 as the genuinely hard rollback slice.
