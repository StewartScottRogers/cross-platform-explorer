---
id: CPE-264
title: Sidecar manifest schema + registry
type: Task
status: Done
priority: High
component: Backend
estimate: 2-3h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Implemented in a new standalone `sidecar/host` crate (`sidecar-host`, depends only
on the contract). `SidecarManifest` (schema_version, id, name, version, target
contract_version, per-OS `entry`, requested `capabilities`, optional `ui` mount) +
`Registry::load_from_dirs` scanning bundled + user dirs in order, user overriding
bundled by id. Malformed JSON, unknown-future `schema_version`, incompatible
contract major, and empty id/entry are **skipped with a recorded `LoadWarning`**,
never fatal; a missing dir contributes nothing. `entry_for_current_os()` resolves
the per-OS launch command. 6 unit tests + clippy clean.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented + tested (6 tests, clippy clean) in the new sidecar-host
crate. Done.
