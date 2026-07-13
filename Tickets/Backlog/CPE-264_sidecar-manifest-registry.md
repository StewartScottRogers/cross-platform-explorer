---
id: CPE-264
title: Sidecar manifest schema + registry
type: Task
status: Open
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
---

## Summary

Discovery for sidecars. A declarative manifest describes each sidecar (id, name,
version, target contract version, entry binary/args per-OS, capabilities
requested, UI-mount info). The registry scans a **bundled** dir and a
**user-writable** dir so new Mega-Features are added as data, not code.

## Acceptance Criteria

- [ ] Manifest schema (`sidecar.json`) with the fields above, validated on load.
- [ ] Registry loads bundled + user manifests; user overrides/extends bundled.
- [ ] Malformed/incompatible manifests are skipped with a logged reason, never
      fatal (mirrors the explorer's skip-on-error listing rule).
- [ ] Tests: valid manifest, missing fields, per-OS entry resolution.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-262]]. **Phase:** P1. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
