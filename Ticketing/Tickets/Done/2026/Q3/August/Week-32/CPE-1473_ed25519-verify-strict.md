---
id: CPE-1473
title: "Use ed25519 verify_strict for sidecar manifest & catalog signatures (defense-in-depth)"
type: Bug
status: Done
priority: Low
component: Backend
tags: [ready, security]
epic: CPE-862
created: 2026-08-08
---
## Vector (found in the crypto/IPC deep audit, 2026-08-08)
`sidecar/host/src/trust.rs:~50` `key.verify(msg, &sig)` (consumed by `catalog::verify_index`/`apply_bundle`) uses
NON-strict ed25519 verification, which accepts non-canonical/malleable signatures.

## Reachability / severity
INFO / hardening only — NOT exploitable here: acceptance is gated by content SHA-256 + monotonic version, and
signature malleability cannot forge a signature for a message an attacker lacks one for. `verify_strict` is the
recommended default (rejects small-order/non-canonical points) — worth adopting as defense-in-depth.

## Fix direction
Switch `key.verify(...)` → `key.verify_strict(...)` at trust.rs:~50 (and any sibling ed25519 verify). Confirm the
existing signature-verify tests still pass; add a case with a non-canonical/malleable signature that verify_strict
rejects if easy to construct.

## Effort / blast radius
XS / one-liner + test. Epic CPE-862. Can batch with the sidecar cluster (CPE-1471/1472) but different file
(trust.rs vs supervisor.rs) — same worker is fine.

## Work Log

- 2026-08-08 — Fixed.

  Switched `key.verify(msg, &sig)` → `key.verify_strict(msg, &sig)` at `sidecar/host/src/trust.rs:50`
  (`verify_signature`, the host's manifest/catalog trust engine). Grepped the whole `sidecar/` tree for
  sibling `.verify(` calls on an ed25519 `VerifyingKey` and found two more, both switched the same way:
  - `sidecar/ai-console/src/catalog.rs:29` (`verify_manifest` — the sidecar-local agent-catalog verifier,
    CPE-308/371, deliberately format-compatible with `trust.rs`).
  - `sidecar/ai-console/src/model_snapshot.rs:137` (`verify_snapshot` — the signed model-catalog-snapshot
    verifier, CPE-450/451).

  `sidecar/host/src/catalog.rs::verify_index` calls `trust::verify_signature` rather than the ed25519 API
  directly, so it picks up the hardening automatically — no separate change needed there.

  Each of the three call sites imported `ed25519_dalek::Verifier` only for the now-removed `.verify(...)`
  call (`verify_strict` is an inherent `VerifyingKey` method, not part of the `Verifier` trait), so the now-
  unused `Verifier` import was removed from each file's `use` statement to keep `-D warnings` clean.
  `model_snapshot.rs` still imports `Signer` (used elsewhere in the same file to *produce* signatures), so
  only `Verifier` was dropped there.

  Did not add a hand-constructed non-canonical/malleable-signature test — genuinely non-canonical S/small-
  order-point test vectors aren't "easy to construct" by hand without a dedicated crafting helper, and the
  ticket flagged that case as optional ("if easy"). Instead confirmed no regression the safe way: every
  existing positive/negative signature-verify test (host `trust.rs`, host `catalog.rs`, ai-console
  `catalog.rs`, ai-console `model_snapshot.rs`) still passes unchanged under `verify_strict`, which is the
  meaningful regression risk (strict verification is a strict superset of rejections, so anything it accepts
  the old code also accepted; the only way this could regress behavior is a previously-accepted signature
  now failing, and none did).

  **Verification:**
  - `cargo build` + `cargo clippy --all-targets -- -D warnings` (sidecar/host) — clean.
  - `cargo test` (sidecar/host) — 102 passed incl. all `trust::tests::*` and `catalog::tests::*`.
  - `cargo build` + `cargo clippy --all-targets -- -D warnings` (sidecar/ai-console) — clean.
  - `cargo test --lib` (sidecar/ai-console) — 381 passed, 2 ignored, incl. all `catalog::tests::*` and
    `model_snapshot::tests::*`.

  PR: branch `cpe-1471-sidecar-ipc-hardening`, bundled with CPE-1471/CPE-1472 (same audit sweep).
