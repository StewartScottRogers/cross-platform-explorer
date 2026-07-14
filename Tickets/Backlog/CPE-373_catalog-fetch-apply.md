---
id: CPE-373
title: "Catalog fetch + apply: host-mediated, offline-safe, last-known-good (CPE-308 part 2, slice 2)"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [ready]
estimate: 3-4h
created: 2026-07-14
---

## Summary

Deliver a catalog **bundle** (`index.json` + `index.json.sig` + manifests + per-manifest `.sig`)
into the sidecar's verified source, applying the CPE-372 gate. Works from a **local/configured
source first** (air-gapped / enterprise "point at your own"), so it's fully testable without a
hosted first-party URL; the remote HTTP fetch is a thin wrapper on top, host-mediated and
proxy/offline-aware (reuse CPE-347/369 — the host holds the URL, no SSRF).

## Acceptance Criteria

- [ ] Host loads + verifies an index (`catalog::verify_index`), gates each entry
      (`gate_manifest` vs a persisted installed-version map), and stages accepted manifests +
      their `.sig`s into the sidecar catalog dir (`CPE_AICONSOLE_CATALOG`).
- [ ] **Last-known-good:** a failed/partial/unverified apply never degrades the working catalog;
      the previous good set + version map persist.
- [ ] Offline-safe: `CPE_OFFLINE` disables any network refresh; the persisted catalog still loads.
- [ ] Remote fetch is host-mediated (host owns the configured URL), proxy/offline-aware, TLS.
- [ ] Tests: apply-accepts-upgrade, rejects rollback/tamper/unlisted, last-known-good on failure,
      version-map persistence. clippy clean.

## Notes
Depends on [[CPE-372]]. The first-party **default source URL** is still an open question (who hosts
it); until then this runs against a local/configured source. Feeds [[CPE-371]]'s `load_signed_source`.

## Work Log
2026-07-14 — Filed as slice 2 of CPE-308 part 2.
