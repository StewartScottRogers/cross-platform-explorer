//! Secrets broker capability provider (CPE-268).
//!
//! Brokered access to the OS secret store, scoped to the requesting sidecar's **own
//! namespace**. A sidecar can set/get/delete named secrets; it never sees the raw
//! store, another sidecar's secrets, or a plaintext file. The service name embeds the
//! (broker-supplied) sidecar id, so a sidecar cannot address another's namespace.
//!
//! Secret VALUES are returned only to the requesting sidecar process (over the local,
//! authenticated IPC, for injection into the child it spawns). They are never sent to
//! the host UI and must never be logged — the redaction utility (CPE-298) covers any
//! diagnostic path; this module simply never logs values.

use serde_json::{json, Value};
use sidecar_contract::{Capability, ContractError, ErrorCode, Request};

use crate::broker::CapabilityProvider;

/// The credential-store backend. Abstracted so the provider is testable with an
/// in-memory store and backed by the OS keychain in production.
pub trait SecretBackend: Send + Sync {
    fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String>;
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String>;
    fn delete(&self, service: &str, account: &str) -> Result<(), String>;
}

/// Serves `Capability::Secrets`. Methods: `secrets.set` {name,value},
/// `secrets.get` {name} → {value}, `secrets.delete` {name}.
pub struct SecretsProvider<B: SecretBackend> {
    backend: B,
}

impl<B: SecretBackend> SecretsProvider<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Keychain "service" for a sidecar — embeds its id so namespaces never collide
    /// or cross.
    fn service_for(sidecar_id: &str) -> String {
        format!("com.cross-platform-explorer.sidecar.{sidecar_id}")
    }

    fn name_param(request: &Request) -> Result<String, ContractError> {
        request
            .params
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| ContractError::new(ErrorCode::ToolFailure, "missing 'name'", false))
    }

    fn to_err(e: String) -> ContractError {
        // Never include a secret value; backend errors are about the store, not values.
        ContractError::new(ErrorCode::Internal, e, false)
    }
}

impl<B: SecretBackend> CapabilityProvider for SecretsProvider<B> {
    fn capability(&self) -> Capability {
        Capability::Secrets
    }

    fn handle(&self, sidecar_id: &str, request: &Request) -> Result<Value, ContractError> {
        let service = Self::service_for(sidecar_id);
        match request.method.as_str() {
            "secrets.set" => {
                let name = Self::name_param(request)?;
                let value = request.params.get("value").and_then(Value::as_str).ok_or_else(|| {
                    ContractError::new(ErrorCode::ToolFailure, "missing 'value'", false)
                })?;
                self.backend.set(&service, &name, value).map_err(Self::to_err)?;
                Ok(json!({ "ok": true }))
            }
            "secrets.get" => {
                let name = Self::name_param(request)?;
                let value = self.backend.get(&service, &name).map_err(Self::to_err)?;
                // `value` is null when absent; returned only to the requesting sidecar.
                Ok(json!({ "value": value }))
            }
            "secrets.delete" => {
                let name = Self::name_param(request)?;
                self.backend.delete(&service, &name).map_err(Self::to_err)?;
                Ok(json!({ "ok": true }))
            }
            other => Err(ContractError::new(
                ErrorCode::ToolFailure,
                format!("unknown secrets method '{other}'"),
                false,
            )),
        }
    }
}

/// The real OS keychain backend (Windows Credential Manager here). macOS/Linux use
/// the same `keyring` API once their store features are enabled in Cargo.toml.
#[cfg(windows)]
pub struct KeyringBackend;

#[cfg(windows)]
impl SecretBackend for KeyringBackend {
    fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
        keyring::Entry::new(service, account)
            .and_then(|e| e.set_password(secret))
            .map_err(|e| e.to_string())
    }

    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        match keyring::Entry::new(service, account).and_then(|e| e.get_password()) {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        match keyring::Entry::new(service, account).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// An in-memory credential store for tests, keyed by (service, account).
    #[derive(Default)]
    struct MemBackend {
        map: Mutex<HashMap<(String, String), String>>,
    }
    impl SecretBackend for MemBackend {
        fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
            self.map
                .lock()
                .unwrap()
                .insert((service.into(), account.into()), secret.into());
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

    fn req(method: &str, params: Value) -> Request {
        Request { method: method.into(), params }
    }

    #[test]
    fn set_then_get_round_trips_for_the_owning_sidecar() {
        let p = SecretsProvider::new(MemBackend::default());
        p.handle("s1", &req("secrets.set", json!({ "name": "openrouter", "value": "sk-xyz" })))
            .unwrap();
        let got = p.handle("s1", &req("secrets.get", json!({ "name": "openrouter" }))).unwrap();
        assert_eq!(got["value"], json!("sk-xyz"));
    }

    #[test]
    fn get_missing_returns_null() {
        let p = SecretsProvider::new(MemBackend::default());
        let got = p.handle("s1", &req("secrets.get", json!({ "name": "nope" }))).unwrap();
        assert_eq!(got["value"], Value::Null);
    }

    #[test]
    fn delete_removes_the_secret() {
        let p = SecretsProvider::new(MemBackend::default());
        p.handle("s1", &req("secrets.set", json!({ "name": "k", "value": "v" }))).unwrap();
        p.handle("s1", &req("secrets.delete", json!({ "name": "k" }))).unwrap();
        let got = p.handle("s1", &req("secrets.get", json!({ "name": "k" }))).unwrap();
        assert_eq!(got["value"], Value::Null);
    }

    #[test]
    fn one_sidecar_cannot_read_anothers_secret() {
        let p = SecretsProvider::new(MemBackend::default());
        p.handle("alpha", &req("secrets.set", json!({ "name": "shared", "value": "A" }))).unwrap();
        // Same name, different sidecar id → different namespace → not found.
        let got = p.handle("beta", &req("secrets.get", json!({ "name": "shared" }))).unwrap();
        assert_eq!(got["value"], Value::Null);
    }

    #[test]
    fn missing_name_or_value_is_a_clean_error() {
        let p = SecretsProvider::new(MemBackend::default());
        assert!(p.handle("s1", &req("secrets.get", json!({}))).is_err());
        assert!(p.handle("s1", &req("secrets.set", json!({ "name": "k" }))).is_err());
    }

    // Real OS keychain round-trip. Ignored by default so it doesn't touch the
    // machine's credential store during normal runs; run with:
    //   cargo test --package sidecar-host -- --ignored keyring_backend_round_trip
    #[cfg(windows)]
    #[test]
    #[ignore = "touches the real Windows Credential Manager"]
    fn keyring_backend_round_trip() {
        let b = KeyringBackend;
        let svc = format!("cpe-test-{}", std::process::id());
        b.set(&svc, "acct", "secret-value").unwrap();
        assert_eq!(b.get(&svc, "acct").unwrap().as_deref(), Some("secret-value"));
        b.delete(&svc, "acct").unwrap();
        assert_eq!(b.get(&svc, "acct").unwrap(), None);
    }
}
