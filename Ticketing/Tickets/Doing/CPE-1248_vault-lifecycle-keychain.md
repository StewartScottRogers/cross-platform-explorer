---
id: CPE-1248
title: "Vault lifecycle + OS-keychain seam + thin async commands (create/lock/unlock/status)"
type: Task
priority: Medium
component: Multiple
tags: [ready, security-sensitive]
estimate: 3h
created: 2026-08-01
epic: CPE-738
closed:
---

## Context
Second slice of the encrypted-vaults half of CPE-738. Builds on the merged crypto core
`cpe_server::vault_crypto` (CPE-1247: `encrypt_tree(root, &SecretString) -> Vec<u8>`,
`decrypt_tree(blob, &SecretString, out_dir)`, `VaultError`). This slice adds the **lifecycle state
model**, the **OS-keychain seam** for passphrase storage, and the **thin async Tauri commands** — but
NOT the tree/browse mount (CPE-1249) or the UI (CPE-1250).

Reuse (grep-first, confirmed):
- Keychain: `keyring` v3 is already a workspace dep; mirror the proven real backend in
  `sidecar/host/src/providers/secrets.rs` (`KeyringBackend`: `keyring::Entry::new(service, account)`
  `.set_password/.get_password/.delete_credential`, mapping `Error::NoEntry` → `Ok(None)`).
- Seam pattern: `sidecar/ai-console/src/vault.rs`'s `SecretAccess` trait (unit-testable via an in-memory
  fake) — mirror that shape so `cpe-server` stays Tauri-free + testable.
- Secure delete: `cpe_server::secure_shred::shred_file` (CPE-1012) is already built for the "seal then
  destroy the plaintext original" path.

## What to build
1. **`crates/server/src/vault_manager.rs`** — pure lifecycle over the crypto core, with a `SecretAccess`
   trait (`set/get/delete(service,account)`), an in-memory fake for tests, and:
   - `is_vault(path) -> bool` — a `.cpevault` detected by reading the `CPEVLT1` magic (not just the
     extension).
   - `create_vault(folder, dest_blob_path, passphrase, opts)`:
     encrypt the folder → write the `.cpevault` blob. **Safety invariant (critical):** if
     `opts.shred_original` is set, VERIFY the blob is decryptable (a round-trip check) BEFORE shredding
     the plaintext via `shred_file` — NEVER destroy the original until the encrypted copy is provably
     recoverable. Default `shred_original = false`.
   - `unlock_vault(blob_path, passphrase, session_dir)`: decrypt into a caller-provided session dir,
     record unlocked state (blob path → session dir). (The crypto core already extracts atomically.)
   - `lock_vault`: drop the unlocked state and **securely wipe** the session dir contents (reuse
     `shred_file` for the extracted files, then remove the dir) so plaintext doesn't linger. Document
     honestly that while unlocked, plaintext lives in the session dir on disk (the mount tradeoff; a
     future in-memory/FUSE mount could avoid it).
   - `remember_passphrase` / `forget_passphrase` / `stored_passphrase` via the `SecretAccess` seam
     (service e.g. `"cpe.vault"`, account = a stable id for the vault, e.g. a hash of the blob path).
     The keychain is the ONLY place a passphrase may persist — never a plaintext file or log.
   - A `VaultStatus` (specta `Type`) the UI can render: is-vault, locked/unlocked, has-stored-passphrase.
2. **Thin Tauri commands** in `src-tauri/src/lib.rs` (one-line dispatchers, registered in
   `generate_handler!`/`collect_commands!`): `vault_is`, `vault_create`, `vault_unlock`, `vault_lock`,
   `vault_status`, `vault_remember_passphrase`, `vault_forget_passphrase`. **All async + `spawn_blocking`**
   (crypto/scrypt ~1s + fs — CPE-760/761). Passphrases arrive as `String` over IPC → wrap into
   `age::secrecy::SecretString` at the boundary (the manager takes `&SecretString`). A real
   `KeyringBackend` impl (mirror the sidecar) lives in the app adapter, not cpe-server.
   Manage unlocked-session state in a Tauri-managed registry (like `TerminalDockState`/`PtyRegistry`).

## Acceptance criteria
- cargo tests (cpe-server, in-memory `SecretAccess` fake, no real keychain): create→unlock→lock
  round-trips a folder; `create_vault` with `shred_original` refuses to shred if the round-trip verify
  fails (inject a failure) and only shreds after a good verify; `lock_vault` leaves no plaintext behind
  (session dir gone); `is_vault` detects by magic and rejects a non-vault file; remember/stored/forget
  passphrase works through the fake.
- `spawn_blocking` used for every fs/crypto command (grep the commands — no blocking call on the async
  thread). Bindings regenerated (`bindings.gen.ts`) for the new `VaultStatus` type + commands
  ([[regen-specta-bindings-on-struct-change]]) — the drift guard must pass.
- `cargo test`, `cargo build`, `cargo clippy --all-targets -D warnings` BOTH feature modes; `npm run check`.
- No plaintext passphrase to disk/log anywhere; no new deps beyond `age`/`keyring` (both already present).

## Out of scope (later)
Tree indicator + browse-the-unlocked-vault-as-a-location (CPE-1249); all UI/dialogs (CPE-1250); the
security-review doc (CPE-1251).

## Notes
Keep the destructive "shred original" OFF by default and gated behind the verify-first invariant — the
confirm UX comes in CPE-1250, but the backend must be safe even if called directly.
