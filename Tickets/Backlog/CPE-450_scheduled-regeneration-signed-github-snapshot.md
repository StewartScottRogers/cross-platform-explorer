---
id: CPE-450
title: "Scheduled regeneration -> signed GitHub snapshot"
type: Feature
status: Open
priority: Medium
component: CI
tags: [needs-prereq]
estimate: 3-4h
created: 2026-07-15
epic: CPE-444
---

## Summary
A scheduled job regenerates a normalized, signed model-catalog snapshot from every reseller and publishes it to GitHub Releases, so clients load fast + offline. Reuses the agent-catalog signing/anti-rollback pipeline (CPE-308/376/371).

## Acceptance Criteria
- [ ] A workflow (scheduled + manual) fetches every reseller's model list, normalizes, and builds one catalog bundle.
- [x] The bundle is ed25519-signed and content-hashed with a strictly-monotonic version (anti-rollback). *(Signing/hash/anti-rollback core landed in `model_snapshot`; publishing to Releases is the remaining workflow step.)*
- [x] Signs with an ed25519 seed hex (the EXISTING `CPE_CATALOG_SIGNING_KEY` shape) — `sign_snapshot(seed_hex, &snapshot)` mirrors `catalog-sign`; no new key needed. *(Wiring the actual secret into a CI job remains.)*
- [ ] Documented cadence + how to run it manually.

## Work Log

- 2026-07-15 — Landed the **signing half of the model-snapshot core** on branch
  `CPE-450-451-model-snapshot-core`: new `sidecar/ai-console/src/model_snapshot.rs` with
  `ModelSnapshot { version, generated_at, models }`, `canonical_bytes` (order-independent,
  deterministic bytes to sign), `content_hash` (hex SHA-256), and `sign_snapshot(seed_hex, &snap)`
  producing a detached ed25519 hex signature. Mirrors `sidecar_host::catalog::sign_bundle` and reuses
  the proven crypto (`ed25519-dalek`, `sha2`, `hex`). Unit-tested (8 tests): sign→verify round-trip,
  order-independent canonical bytes, malformed-seed rejection without panic. `cargo test
  model_snapshot` and `cargo clippy --all-targets -D warnings` both clean.
- **Still open (runtime/CI follow-ups, deliberately not built here):** the scheduled + manual
  **GitHub Actions workflow** that fetches every reseller's live model list, normalizes, builds the
  bundle, signs it with the real `CPE_CATALOG_SIGNING_KEY` secret, and publishes it to Releases; plus
  the documented cadence / manual-run instructions. Ticket stays in Backlog.

## Notes
**Not key-gated.** Correction (2026-07-15): the catalog signing key already exists — `CPE_CATALOG_SIGNING_KEY`
is a set repo secret and its public key is embedded in `CATALOG_TRUSTED_KEYS` (CPE-380); release v0.13.0
already ships a signed *agent* catalog bundle. This ticket reuses that same key + the `catalog-sign`
machinery for the *model* snapshot — so it's a build task (the model-snapshot job), not a key-procurement
gate. Retag to `ready` when picked up. `needs-prereq` now only reflects CPE-445..448 (the model data),
which are DONE — so this is effectively actionable.
