//! Connection-secret store (CPE-1510, epic CPE-1497 "mount anything" F1 slice).
//!
//! A saved remote-[`crate::connections::Connection`] profile records only non-secret metadata (host,
//! port, user, auth *method*, and for key auth the private-key *path*) — see `connections.rs`'s module
//! docs. The actual password / key passphrase is never written to `connections.json`; it lives in the OS
//! keychain, keyed by the connection's stable `name`, and is fetched here at connect time for
//! [`crate::vfs`]'s `open(conn, secret, known_hosts, policy)` (CPE-1499 wires that call; this module only
//! makes the secret storable/retrievable).
//!
//! # Reuses the existing keychain seam
//! Rather than a second `SecretBackend` trait, this reuses [`crate::vault_manager::SecretAccess`] — the
//! same brokered-keychain seam [`crate::vault_manager`] uses for vault passphrases, and the app adapter
//! already uses for the content-search embedder key and the AI-copilot key (`src-tauri/src/lib.rs`). It's
//! already exactly this module's shape (`set`/`get`/`delete` over `(service, account)`, Tauri-free,
//! backed by `keyring` in production and an in-memory fake in tests), so a parallel trait would just be
//! the same abstraction under a different name. `SecretAccess` lives in `vault_manager` for historical
//! reasons (it shipped there first); this module simply names its own **service** —
//! [`CONNECTION_SECRET_SERVICE`] — so connection secrets never share a keychain namespace with vault
//! passphrases, the embedder key, the copilot key, or (a separate credential store entirely) the
//! sidecar's own per-sidecar AI-key namespace.
//!
//! # Tauri-free by construction
//! Like `vault_manager`, this module never touches Tauri or the `keyring` crate directly — the real
//! `keyring`-backed `SecretAccess` impl lives in the app adapter (`src-tauri`'s `KeyringBackend`, already
//! built for the vault/embedder/copilot work); tests here run entirely against an in-memory fake, so
//! `cargo test` never touches the real OS credential store.
//!
//! **Never store plaintext on disk** — there is no file-backed path in this module at all; every secret
//! value flows only through the [`SecretAccess`] seam into the OS keychain.

use crate::vault_manager::SecretAccess;

/// Keychain "service" every connection secret is stored under; the connection's `name` (from
/// `connections.rs`) is the per-secret "account". Distinct from [`crate::vault_manager::VAULT_SERVICE`]
/// (`"cpe.vault"`) and the app adapter's `"cpe.content-embedder"` / `"cpe.copilot"` services, and from the
/// sidecar's own `com.cross-platform-explorer.sidecar.{id}` namespace — a connection secret can never
/// collide with, or be read back as, any other kind of stored secret.
pub const CONNECTION_SECRET_SERVICE: &str = "cpe-connection";

/// Save `secret` (a password or key passphrase) for the connection named `name`, overwriting any value
/// already stored for that name. This is the ONLY place a connection secret persists.
pub fn set_secret(access: &dyn SecretAccess, name: &str, secret: &str) -> Result<(), String> {
    access.set(CONNECTION_SECRET_SERVICE, name, secret)
}

/// Fetch the stored secret for the connection named `name`, or `Ok(None)` if none is saved.
pub fn get_secret(access: &dyn SecretAccess, name: &str) -> Result<Option<String>, String> {
    access.get(CONNECTION_SECRET_SERVICE, name)
}

/// Delete the stored secret for the connection named `name`. Deleting a missing entry is `Ok`.
pub fn delete_secret(access: &dyn SecretAccess, name: &str) -> Result<(), String> {
    access.delete(CONNECTION_SECRET_SERVICE, name)
}

/// The helper the future CPE-1499 connect-path wiring calls to resolve a saved connection's secret
/// before handing it to `vfs::open(conn, secret, known_hosts, policy)`. Currently identical to
/// [`get_secret`] — a distinct name so call sites read as "resolve this connection's secret" rather than
/// a bare store lookup, and so the lookup strategy (e.g. a future prompt-if-absent fallback) has one seam
/// to change.
pub fn secret_for(access: &dyn SecretAccess, name: &str) -> Result<Option<String>, String> {
    get_secret(access, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory keychain fake keyed by `(service, account)` — mirrors the fakes in `vault_manager`'s
    /// and the sidecar's own secrets tests, so no real credential store is touched here.
    #[derive(Default)]
    struct MemAccess {
        map: Mutex<HashMap<(String, String), String>>,
    }
    impl SecretAccess for MemAccess {
        fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
            self.map.lock().unwrap().insert((service.into(), account.into()), secret.into());
            Ok(())
        }
        fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
            Ok(self.map.lock().unwrap().get(&(service.into(), account.into())).cloned())
        }
        fn delete(&self, service: &str, account: &str) -> Result<(), String> {
            self.map.lock().unwrap().remove(&(service.into(), account.into()));
            Ok(())
        }
    }

    #[test]
    fn set_then_get_round_trips() {
        let a = MemAccess::default();
        set_secret(&a, "prod", "s3kr3t").unwrap();
        assert_eq!(get_secret(&a, "prod").unwrap().as_deref(), Some("s3kr3t"));
    }

    #[test]
    fn get_missing_is_none() {
        let a = MemAccess::default();
        assert_eq!(get_secret(&a, "nope").unwrap(), None);
    }

    #[test]
    fn delete_removes_the_secret() {
        let a = MemAccess::default();
        set_secret(&a, "prod", "s3kr3t").unwrap();
        delete_secret(&a, "prod").unwrap();
        assert_eq!(get_secret(&a, "prod").unwrap(), None);
    }

    #[test]
    fn delete_of_a_missing_entry_is_ok() {
        let a = MemAccess::default();
        assert!(delete_secret(&a, "never-set").is_ok());
    }

    #[test]
    fn set_overwrites_the_previous_value() {
        let a = MemAccess::default();
        set_secret(&a, "prod", "first").unwrap();
        set_secret(&a, "prod", "second").unwrap();
        assert_eq!(get_secret(&a, "prod").unwrap().as_deref(), Some("second"));
    }

    #[test]
    fn two_connection_names_are_isolated() {
        let a = MemAccess::default();
        set_secret(&a, "prod", "prod-secret").unwrap();
        set_secret(&a, "staging", "staging-secret").unwrap();
        assert_eq!(get_secret(&a, "prod").unwrap().as_deref(), Some("prod-secret"));
        assert_eq!(get_secret(&a, "staging").unwrap().as_deref(), Some("staging-secret"));
        // Deleting one never touches the other.
        delete_secret(&a, "prod").unwrap();
        assert_eq!(get_secret(&a, "prod").unwrap(), None);
        assert_eq!(get_secret(&a, "staging").unwrap().as_deref(), Some("staging-secret"));
    }

    #[test]
    fn store_never_returns_another_services_secret() {
        // Same account, different service — proves the connection namespace can't read back a secret
        // stored under a different service (e.g. the vault or embedder-key namespace).
        let a = MemAccess::default();
        a.set("cpe.vault", "shared-name", "vault-value").unwrap();
        assert_eq!(get_secret(&a, "shared-name").unwrap(), None);
        set_secret(&a, "shared-name", "connection-value").unwrap();
        assert_eq!(
            a.get("cpe.vault", "shared-name").unwrap().as_deref(),
            Some("vault-value"),
            "the other service's secret must be untouched"
        );
        assert_eq!(get_secret(&a, "shared-name").unwrap().as_deref(), Some("connection-value"));
    }

    #[test]
    fn secret_for_is_the_get_secret_lookup() {
        let a = MemAccess::default();
        set_secret(&a, "prod", "s3kr3t").unwrap();
        assert_eq!(secret_for(&a, "prod").unwrap().as_deref(), Some("s3kr3t"));
        assert_eq!(secret_for(&a, "absent").unwrap(), None);
    }
}
