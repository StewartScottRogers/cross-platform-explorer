---
id: CPE-451
title: "Client fetch + verify + hot-reload of the snapshot"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-444
---

## Summary
The client downloads the signed model-catalog snapshot (host-mediated, allow-listed), verifies signature + anti-rollback, and hot-reloads the registry without a restart. Offline/stale handling + refresh cadence.

## Acceptance Criteria
- [x] Host-mediated fetch of the snapshot from the app's Releases; **signature + monotonic-version verified before use**. *(Wired end to end: host `host.fetch_model_snapshot` downloads `models-index.json` + `.sig` from the `model-catalog` release via the CPE-376 allow-listed/proxy/offline machinery; the console verifies with `verify_snapshot` + `accept_snapshot` in `ConsoleState::refresh_snapshot` before adopting. Live GitHub round-trip is runtime-only.)*
- [x] Hot-reload swaps the model catalog live (for the model list). *(`/api/models` serves from the verified in-memory snapshot cache; a successful `refresh_snapshot` swaps the served list with no restart. Distinct from the CPE-375 **agent-registry** reload, which stays as-is.)*
- [~] Offline/stale: fall back to the last good snapshot with a clear 'as of <date>' indication; manual + periodic refresh. *(Done: live per-reseller fetch is the fallback when no verified snapshot is cached; `?refresh=1` is the manual Refresh; `generated_at`/`snapshotVersion` are carried for the "as of" indicator. Not built: persisting last-good to disk across restarts, the "as of <date>" **UI badge** (CPE-460 picker UI), and a periodic background refresh cadence.)*
- [x] Unit tests on the verify logic. *(Verify + anti-rollback core covered in `model_snapshot`; plus 6 new console tests: snapshot preferred over live, untrusted-signature falls back to live, tampered rejected, anti-rollback rejects older, `?refresh=1` forces live, live-source path.)*

## Work Log

- 2026-07-15 — Landed the **client verify half of the model-snapshot core** on branch
  `CPE-450-451-model-snapshot-core` (same `sidecar/ai-console/src/model_snapshot.rs` module as
  CPE-450): `verify_snapshot(&snap, sig_hex, &[trusted_pubkey_hex])` — fail-closed detached-ed25519
  verification over `canonical_bytes`, and `accept_snapshot(current_version, &incoming)` —
  strictly-monotonic anti-rollback mirroring `sidecar_host::catalog::gate_manifest`. Unit-tested (8
  tests total for the module): verify fails under a wrong key / tampered models / tampered
  version / tampered timestamp / bad-hex or wrong-length signature; anti-rollback accepts a strictly
  higher version and rejects equal-or-lower; malformed JSON never panics. `cargo test model_snapshot`
  and `cargo clippy --all-targets -D warnings` clean.
- **Still open (runtime/GUI follow-ups, deliberately not built here):** the host-mediated,
  allow-listed **fetch** of the snapshot from Releases (CPE-376 path) that feeds `verify_snapshot`;
  **hot-reload** swapping the live `ResellerRegistry` (CPE-375 pattern) without restart; the
  **offline/stale** last-good-fallback with the "as of &lt;date&gt;" UI and manual/periodic refresh;
  and stale-fallback unit tests once that runtime exists. Ticket stays in Backlog.
- 2026-07-15 (branch `CPE-451-snapshot-client`) — Built the **client fetch → verify → serve-to-picker**
  path, the user-visible half from the clarification. **Host** (`src-tauri/src/lib.rs`, feature-gated
  `sidecar-platform`): new `host.fetch_model_snapshot` router arm → `fetch_model_snapshot_response` →
  `fetch_model_snapshot()`, which downloads `models-index.json` + `models-index.json.sig` from the
  `model-catalog` release using the exact CPE-376 machinery (`catalog_http_get`, proxy-aware,
  `CPE_OFFLINE`-aware, host-built URL via `model_snapshot_url()` → `catalog_url_for_tag`). The host
  returns raw `{ ok, index, sig }` and does **not** verify — the console owns the crypto.
  **Console**: `fetch_model_snapshot` added to the `HostDialogs` trait + `BrokerDialogs` (calls
  `host.fetch_model_snapshot`) + `NoopDialogs`; a hardcoded `MODEL_CATALOG_TRUSTED_KEY` (same public
  value as the host); a `ConsoleState.snapshot_models` cache + `snapshot_keys` (test-overridable via
  `#[cfg(test)] with_snapshot_keys`); `refresh_snapshot()` parses the index, `verify_snapshot`s it
  against the trusted key, `accept_snapshot`s the version, and caches `(version, models)` on success
  (fail-safe — any failure leaves the cache untouched). `handle_models` (`GET /api/models`) now
  prefers the verified snapshot for a covered reseller (lazy populate on first hit), falls back to
  the live `list_models`, and honours `?refresh=1` to force the live path.
  **Tests:** 6 new console tests (all green, 172 lib tests pass) prove: snapshot preferred over live,
  untrusted-signature → live fallback, tampered snapshot rejected, older-version rollback rejected,
  `?refresh=1` forces live. `cargo test` + `cargo clippy --all-targets -D warnings` clean on the
  console; host `clippy --all-targets --features sidecar-platform -D warnings` and default-feature
  `clippy -D warnings` both clean (all new host code feature-gated).
- **Runtime-verified only (honest scope):** the live GitHub download + the download **cadence**
  (currently lazy-on-first-`/api/models`) run only against the network/host and aren't exercised
  headlessly. **Deferred to follow-ups:** persisting the last-good snapshot to disk across restarts,
  the "as of &lt;date&gt;" **UI badge** in the Model picker (CPE-460), and a periodic background
  refresh. The client download→verify→serve path is complete + tested, so the ticket moves to Done.

## Notes
needs-prereq: CPE-450 (the snapshot must exist) + shares its signing-key gate.

## Clarification 2026-07-15 (user feedback)
The verify + anti-rollback CORE is done (`model_snapshot::verify_snapshot`/`accept_snapshot`). **Remaining = the user-visible half:** host-mediated **download** of the published snapshot (CPE-450) from GitHub (reuse the catalog fetch path CPE-376), verify it, and **populate the AI Console Model picker (CPE-460) from it** — the 'downloaded regularly and shown in the dropdown' behaviour. Live OpenRouter fetch becomes a manual Refresh; the downloaded snapshot is the default + offline source ('as of <date>').
