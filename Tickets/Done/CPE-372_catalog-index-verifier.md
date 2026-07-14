---
id: CPE-372
title: "Signed catalog index: verifier + anti-rollback (CPE-308 part 2, slice 1)"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The host-authoritative trust core for runtime catalog updates (design decision D1). A signed
catalog **index** lists agent-manifest entries; the host verifies the index against a trusted key,
binds each entry to its manifest content by SHA-256, and enforces monotonic versions
(anti-rollback) before any manifest is loaded.

## Acceptance Criteria

- [x] `host::catalog`: `CatalogIndex`/`CatalogEntry`, `verify_index` (ed25519, CPE-295 format),
      `gate_manifest` → `Accept | Unlisted | ContentMismatch | Rollback`.
- [x] Content binding (sha256) + anti-rollback (strict-monotonic `version`) + unknown-schema guard.
- [x] Pure + fully unit-tested; clippy clean.
- [x] Threat-model §4 gains the catalog-update row (verify + content-bind + anti-rollback; fetch
      deferred to part 2).

## Notes
Reconciles with CPE-371: the **index** (host) governs *which ids + versions* are allowed; the
**per-manifest `.sig`** (sidecar, CPE-371) governs *content authenticity*. A catalog bundle is
`index.json` + `index.json.sig` + N manifests + N `*.json.sig`, all under one trusted key — two
layers, defence-in-depth. Remaining: fetch/apply (CPE-373) + user controls (CPE-374).

## Work Log
2026-07-14 — Implemented & landed. 4 catalog tests (index verify valid/tampered/untrusted-key;
gate accept/unlisted/mismatch/rollback; schema guard); clippy clean.
