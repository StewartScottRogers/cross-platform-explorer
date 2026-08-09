---
id: CPE-1510
title: "Connection-secret keychain store — set/get/delete secrets keyed by connection, feed vfs::open"
type: Feature
status: Done
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

## Work Log — 2026-08-08

**Module location: `crates/server/src/secret_store.rs` (cpe-server, Tauri-free).** `cpe-server` already
takes per-OS-gated platform deps cleanly (`winreg`/`junction` under `cfg(windows)`, `xattr` under
`cfg(unix)`), so there was no reason to push this into `src-tauri` — except for `keyring` itself, which
the codebase had *already* decided stays out of `cpe-server` (see below).

**The trait seam: reused, not duplicated.** Before adding a new `SecretBackend` trait as sketched in the
ticket, found that `crates/server/src/vault_manager.rs` already defines exactly this seam —
`pub trait SecretAccess { set/get/delete(service, account) }` — built for CPE-1248 (vault passphrases)
and already reused twice since (the content-embedder key CPE-1273, the AI-copilot key CPE-1275), each
under its own keychain *service* string. `secret_store.rs` reuses `vault_manager::SecretAccess` rather
than introducing a parallel identical trait — same shape, same Tauri-free/testable intent, and the app
adapter's existing `KeyringBackend` (`src-tauri/src/lib.rs`, already implements `SecretAccess` via the
`keyring` crate, already cfg-gated per OS) needed zero changes to serve a fourth caller. Documented the
"why reuse, not a new trait" reasoning directly in `secret_store.rs`'s module doc comment for the next
reader who might be tempted to add a fifth near-identical trait.

**No new Cargo dependency, no Cargo.lock changes anywhere.** `keyring` v3 (with the same per-OS
`windows-native`/`apple-native`/`sync-secret-service,crypto-rust` features as `sidecar/host`) was already
a `src-tauri` dependency from the vault/embedder/copilot work — confirmed via `grep keyring
src-tauri/Cargo.toml`. `cpe-server` itself takes no `keyring` dependency at all (by design — see above),
so nothing was added to `crates/server/Cargo.toml` either. `git status` after the full build/test/clippy
pass shows zero `Cargo.lock` diffs (`src-tauri/Cargo.lock`, `crates/server/Cargo.lock`, or any other).

**Keychain namespace: service `"cpe-connection"`, account = the connection's `name`.** Distinct from
`vault_manager::VAULT_SERVICE` (`"cpe.vault"`), the app adapter's `"cpe.content-embedder"` /
`"cpe.copilot"` services, and entirely separate from the sidecar's own per-sidecar
`com.cross-platform-explorer.sidecar.{id}` namespace — a connection secret can never collide with, or be
read back as, any other stored secret. Covered by a dedicated `store_never_returns_another_services_secret`
test (stores under `"cpe.vault"` directly, proves `secret_store::get_secret` can't see it, and vice versa).

**API:** `set_secret(access, name, secret)`, `get_secret(access, name) -> Option<String>`,
`delete_secret(access, name)`, plus `secret_for(access, name)` — an alias `get_secret` currently, named
distinctly so CPE-1499's future `vfs::open` wiring reads as "resolve this connection's secret" and has one
seam to change if a fallback (e.g. prompt-if-absent) is added later. All four take `&dyn SecretAccess`, so
CPE-1499 can call them with either the real `KeyringBackend` or a test fake.

**Tauri commands (`src-tauri/src/lib.rs`, new section right after the vault commands):**
`connection_secret_set(name, secret)`, `connection_secret_get(name) -> Option<String>`,
`connection_secret_delete(name)` — all `async fn` + `tauri::async_runtime::spawn_blocking` (CPE-760/761),
registered in both `generate_handler![]` and the specta `collect_commands![]` export. Non-keychain
platforms (`cfg(not(any(windows, macos, linux)))`) degrade cleanly: `set` errors "no OS keychain available
on this platform", `get` returns `None`, `delete` is a no-op `Ok(())` — mirrors the existing
`content_embedder_set_key`/`copilot_set_key` fallback shape.

**No plaintext-secret-on-disk path confirmed:** `secret_store.rs` contains no file I/O at all — every
value flows through `SecretAccess` into the OS keychain only. `connections.rs` (unchanged) already
enforces the complementary half: `save_connections`'s own test
(`save_then_load_round_trips_without_secrets`) asserts the on-disk JSON never contains a password.

**Tests — `cargo test --lib` in `crates/server`, all against the in-memory `MemAccess` mock (no real OS
keychain touched): 8/8 new `secret_store::tests::*` pass** (round-trip, get-missing→None, delete, delete-
of-missing, overwrite, two-connection-name isolation incl. delete-doesn't-cross-leak, cross-service
isolation, `secret_for` lookup) — **full crate suite 1797/1797 pass**, 0 failures. No new real-keychain
`#[ignore]`d integration test was added: the shared `src-tauri` `KeyringBackend` impl this module calls
through is the same one `vault_manager`/`content_embedder`/`copilot` already use, and none of *those*
carry a real-keychain `#[ignore]`d test either (only the sidecar's own separate `secrets.rs` has one, for
its own separate `KeyringBackend`) — so adding one only for the connection-secret call path would be
inconsistent, not more thorough.

**Verification:** `cargo build` (`src-tauri`) OK. `cargo clippy --all-targets -- -D warnings` clean for
`crates/server` (default **and** `--features index`) and for `src-tauri` (default **and**
`--features sidecar-platform`). `cargo run --bin export_bindings --features "specta-bindings
sidecar-platform"` regenerated `src/lib/bindings.gen.ts` — diff is exactly the 3 new
`connectionSecretSet/Get/Delete` typed-client methods, nothing else moved; the
`typed_bindings_are_committed_and_routed_through_busy_cursor` drift guard passes.

**Left for CPE-1499 (explicitly out of scope here):** wiring `secret_store::secret_for` into the actual
connect flow that calls `vfs::open(conn, secret, known_hosts, policy)`. **Left for CPE-1498:** the
Network-sidebar "remember this password" UI toggle and the connect-time secret prompt that would call
these three new Tauri commands from the frontend.
