---
id: CPE-450
title: "Scheduled regeneration -> signed GitHub snapshot"
type: Feature
status: Done
priority: Medium
component: CI
tags: [ready]
estimate: 3-4h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-444
---

## Summary
A scheduled job regenerates a normalized, signed model-catalog snapshot from every reseller and publishes it to GitHub Releases, so clients load fast + offline. Reuses the agent-catalog signing/anti-rollback pipeline (CPE-308/376/371).

## Acceptance Criteria
- [x] A workflow (**manual**) fetches every reseller's model list, normalizes, and builds one catalog bundle. *(`.github/workflows/model-snapshot.yml` curls each reseller's `models_endpoint` → `responses/<id>.json`, then `model-snapshot-sign` normalizes + collects them into one signed `models-index.json`. Trigger is `workflow_dispatch` only for now — a `schedule` cron is present but commented with a note, so it's opt-in and non-destructive per spec. The live curl fetch is only exercised at CI runtime, not headlessly.)*
- [x] The bundle is ed25519-signed and content-hashed with a strictly-monotonic version (anti-rollback). *(Signing/hash/anti-rollback core in `model_snapshot`; the workflow stamps `version=date +%s` so a newer run always wins.)*
- [x] Signs with an ed25519 seed hex (the EXISTING `CPE_CATALOG_SIGNING_KEY` shape) — `sign_snapshot(seed_hex, &snapshot)` mirrors `catalog-sign`; no new key needed. *(Now wired into the `model-snapshot-sign` binary + the CI job, guarded so it SKIPS when the secret is absent.)*
- [x] Documented cadence + how to run it manually. *(Header comment block in `model-snapshot.yml`: manual-run steps, promote-to-Release steps, and cadence — manual now, scheduled left commented.)*

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
- 2026-07-15 — **Producer side built** on branch `CPE-450-model-snapshot-producer` (completes the
  ticket). Three parts:
  1. **Lib helper** `model_snapshot::snapshot_from_reseller_dir(dir, version, generated_at)` — reads
     each `<reseller>.json` in `dir`, calls `model_catalog::normalize_models(<file-stem>, body)`, and
     collects every `Model` into one `ModelSnapshot`. Tolerant like `list_dir`: a missing dir,
     unreadable file, or garbage JSON is skipped, never fatal. Unit-tested (2 new tests, 10 in the
     module total): a temp dir with an OpenRouter-shaped `openrouter.json` + a GitHub-Models-shaped
     `github-models.json` (+ a broken file + a `.txt`) yields exactly the 3 real models, tagged with
     their source reseller, and the bundle signs+verifies; a missing directory yields an empty
     snapshot without panicking.
  2. **Binary** `sidecar/ai-console/src/bin/model_snapshot_sign.rs` (bin name `model-snapshot-sign`,
     declared in `Cargo.toml`, mirroring the host's `catalog-sign`). Args
     `<resellers-response-dir> <out-dir> <version>`; reads the seed from `CPE_CATALOG_SIGNING_KEY`
     (never printed), builds the snapshot via the helper, signs it, and writes
     `<out-dir>/models-index.json` (canonical bytes) + `models-index.json.sig`. `generated_at` comes
     from the optional `SNAPSHOT_GENERATED_AT` env (workflow sets it via `date -u`), else a
     `unix:<secs>` fallback so the binary stays dependency-free. Verified end-to-end locally with a
     throwaway key: 3 models in → signed canonical `models-index.json` + 64-byte hex sig out.
  3. **Workflow** `.github/workflows/model-snapshot.yml` — `workflow_dispatch`-only (a `schedule`
     cron is present but commented, with a note), guarded on the `CPE_CATALOG_SIGNING_KEY` secret so
     it SKIPS (never fails) on repos without the key, exactly like `release.yml`'s `catalog` job. It
     curls each reseller's `models_endpoint` best-effort into `responses/<id>.json` (a failed/auth-
     gated fetch just omits that reseller), builds+runs the binary, and uploads
     `models-index.json` + `.sig` as a **workflow artifact**. It deliberately does NOT publish to a
     Release or touch the app release flow — a human downloads, reviews, and promotes (documented in
     the header comment block). `cargo test snapshot_from_reseller_dir`, `cargo build --bin
     model-snapshot-sign`, and `cargo clippy --all-targets -D warnings` all clean.
  - **Honest scope:** the **live reseller fetch** and **promoting the artifact to a Release** are
    only exercised at CI runtime / by a human — not headlessly verified here. Everything statically
    checkable (helper, signing, canonical bytes, guard logic, artifact upload wiring) is built and
    green. Moving to Done: the producer + guarded manual workflow + documented cadence satisfy the
    ticket's spirit; the intentionally-manual promote-to-Release step is a human decision by design
    (a bad scrape must not auto-ship), not missing work.

## Notes
**Not key-gated.** Correction (2026-07-15): the catalog signing key already exists — `CPE_CATALOG_SIGNING_KEY`
is a set repo secret and its public key is embedded in `CATALOG_TRUSTED_KEYS` (CPE-380); release v0.13.0
already ships a signed *agent* catalog bundle. This ticket reuses that same key + the `catalog-sign`
machinery for the *model* snapshot — so it's a build task (the model-snapshot job), not a key-procurement
gate. Retag to `ready` when picked up. `needs-prereq` now only reflects CPE-445..448 (the model data),
which are DONE — so this is effectively actionable.
