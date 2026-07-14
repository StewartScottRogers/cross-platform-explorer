---
id: CPE-373
title: "Catalog apply core: gate a bundle → catalog dir, offline-safe, last-known-good (CPE-308 part 2, slice 2a)"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 2h
created: 2026-07-14
closed: 2026-07-14
---

## Summary

Host-side apply of a staged catalog **bundle** (`index.json` + `.sig` + per-entry `<id>.json` +
`.sig`) into the sidecar catalog dir, gating every entry against the signed index (CPE-372):
content-bound + anti-rollback, offline by construction, last-known-good on any failure. The remote
HTTP fetch that fills the staging dir + the runtime sidecar reload are split to CPE-375.

## Acceptance Criteria

- [x] `catalog::apply_bundle(staging, out, trusted_keys, installed) -> ApplyReport`: verify index,
      gate each entry (`gate_manifest` vs a persisted version map), require a trusted-key-signed
      manifest, write accepted `<id>.json` + `.sig`, bump the version map.
- [x] **Last-known-good:** a bad/missing index writes nothing; a rejected entry leaves its prior
      copy untouched (`index_ok=false` ⇒ zero writes).
- [x] Offline by construction (reads local staging; no network in the apply path).
- [x] `VersionMap` persistence (`load_versions`/`save_versions`) so anti-rollback survives restarts.
- [x] Tests: accept-upgrade + version bump; reject rollback/tamper without touching the good copy;
      bad index sig touches nothing; missing manifest sig rejected; version-map round-trip. clippy clean.

## Notes
Depends on [[CPE-372]]. Remote fetch + runtime reload → [[CPE-375]]. Feeds [[CPE-371]]'s
`load_signed_source` (the sidecar re-verifies each manifest on load — defence-in-depth).

## Work Log
2026-07-14 — Implemented & landed. 9 host catalog tests total (index verify/gate + apply); clippy clean.
