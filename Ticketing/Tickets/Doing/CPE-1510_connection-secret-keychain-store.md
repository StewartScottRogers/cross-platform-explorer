---
id: CPE-1510
title: "Connection-secret keychain store — set/get/delete secrets keyed by connection, feed vfs::open"
type: Feature
status: Doing
priority: Medium
component: Backend
tags: [ready]
epic: CPE-1497
created: 2026-08-08
---
## What (CPE-1497's F1 slice — the prerequisite for connecting to any remote share)
The VFS stack is built: `crates/vfs/src/lib.rs` `vfs::open(conn, secret, known_hosts, policy)` already **takes
the secret as a param** — but nothing fetches/stores it yet. `crates/server/src/connections.rs` stores auth
*method* only, never the secret. Add an **OS-keychain-backed connection-secret store** so a saved connection's
password/passphrase/token can be persisted securely and handed to `vfs::open`.

## How — lift the proven pattern (NO new dep beyond keyring, which is already in the tree)
`sidecar/host/src/providers/secrets.rs` already uses **`keyring` v3** with the per-OS features configured
(Windows Credential Manager / macOS Keychain / Linux Secret Service) for AI-console keys. Lift that pattern
into the main app / `cpe-server`:
- New module (e.g. `crates/server/src/secret_store.rs`): `set_secret(connection_name, secret)`,
  `get_secret(connection_name) -> Option<String>`, `delete_secret(connection_name)` — keyed by the connection
  `name` from `connections.rs`, namespaced (service = e.g. `"cpe-connection"`, account = connection name) so it
  never collides with the sidecar's AI-key namespace. **Never store plaintext on disk** (satisfies CPE-616 DoD).
- Add `keyring` (v3, same version/features as `sidecar/host/Cargo.toml`) to `crates/server/Cargo.toml` — this is
  the ONE justified dep (it's already a tree dependency, not net-new to the workspace; Dependency Steward: it's
  the standard cross-platform OS-keychain crate, already vetted + used in the sidecar). If `crates/server`
  can't take keyring cleanly (e.g. it must stay headless-testable / platform-portable), put the module in the
  Tauri app (`src-tauri`) instead and expose it — decide + document.
- **Testability seam (important):** hitting the real OS keychain in `cargo test`/CI is flaky/unavailable on
  headless Linux CI. Put the keychain access behind a small trait (`SecretBackend`) with a real `keyring` impl
  + an in-memory mock for tests; unit-test the store logic (set→get round-trip, get-missing→None, delete,
  overwrite, namespace isolation) against the mock. Gate any real-keyring integration test behind
  `#[ignore]`/an env flag so CI stays green (mirror how the sidecar tests its secrets, if it does).
- Expose three async `#[tauri::command]`s (spawn_blocking) in `src-tauri/src/lib.rs`
  (`connection_secret_set/get/delete`), registered in `generate_handler![]`; regenerate `bindings.gen.ts` if a
  `specta::Type` is involved (likely just String args → maybe not needed; check).
- Provide the glue so the connect path can fetch the secret and pass it to `vfs::open` (a helper
  `secret_for(connection_name)` the future CPE-1499 wiring calls) — but the actual `vfs::open` routing is
  CPE-1499's job; here just make the secret retrievable.

## Verify (headless)
`cargo test` (crates/server or src-tauri as chosen): store logic via the mock backend (round-trip, missing,
delete, overwrite, namespace isolation) — all pass without touching the real OS keychain. `cargo clippy
--all-targets -D warnings` both feature modes. Confirm no plaintext-secret path. **Regenerate + commit
`src-tauri/Cargo.lock`** if a dep was added (the shipped-app lockfile — the sprint has hit this before).

## Ship
Move this ticket Doing→Done, Work Log the design (where the module lives, the trait seam, the keyring
namespace). The "remember" toggle UI + connect-time secret prompt are CPE-1498's (Network sidebar) — not here.
Backend-heavy, headless-buildable. Effort S–M.
