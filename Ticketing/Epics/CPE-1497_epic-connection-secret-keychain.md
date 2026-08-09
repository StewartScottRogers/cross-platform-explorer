---
id: CPE-1497
title: "EPIC: Network F1 — connection-secret storage in the app (OS keychain)"
type: Task
status: In Progress
priority: Medium
component: Backend
tags: [epic]
epic: CPE-616
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (sprint PM, Network/"mount anything" research — see research-library
> `network-filesharing-program-2026-08-08`).** Part of the **Network Filesharing program** (parent CPE-616
> in-app-VFS arm + CPE-716 OS-mount arm, hybrid). **Foundation epic F1.** Dormant — activate via
> `/ticketing-epic`.

## Why (foundation — protocols can't connect without it)
The provider stack is built (SFTP/WebDAV Done; `vfs::open` already **expects the caller to pass the fetched
secret**) — but **nobody fetches it yet**. `crates/server`/the app have no keychain access; `Connection` stores
auth *method* only. The proven `keyring` v3 pattern already works in `sidecar/host/src/providers/secrets.rs`
(Windows Credential Manager / macOS Keychain / Linux Secret Service) for AI-console keys — **lift it into the
app** for connection secrets.

## Scope
- Bring `keyring` v3 (with the per-OS features already configured in the sidecar) into cpe-server/app; store
  password/passphrase/token keyed by connection name; secret set/get/delete commands; feed `vfs::open`.
- Never plaintext (satisfies CPE-616's DoD). A "remember" toggle at connect time (tiny frontend).

## Effort / deps / fit
S–M (pattern exists to copy). Backend-heavy + tiny frontend. Deps: none (uses existing `connections.rs`).
Purpose-fit: clean; behind the Network feature. Not `vault_manager.rs` (that's file-encryption — unrelated).

## ACTIVATED 2026-08-08 (Sprint, user-directed: "Activate Network SFTP/WebDAV")
Decomposed just-in-time. Confirmed the substrate exists: `sidecar/host/src/providers/secrets.rs` (keyring v3
pattern to lift), `crates/server/src/connections.rs` (secret-free profiles), `crates/vfs/src/lib.rs`
(`vfs::open(conn, secret, ...)` already takes the secret param). First (and likely only) buildable slice:
- **CPE-1510** — connection-secret keychain store (backend: lift keyring into cpe-server, set/get/delete keyed
  by connection name, testable seam). Headless-buildable. The "remember" toggle UI folds into CPE-1498's
  Network sidebar.
Sequence for the program: CPE-1497 (this) → CPE-1498 (Network sidebar) → CPE-1499 (vfs::open command wiring →
SFTP+WebDAV live).
