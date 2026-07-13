//! Secret vault & credential profiles (CPE-279).
//!
//! Provider keys and **environment login profiles** for the AI Console. A
//! [`CredentialProfile`] is a named, switchable set of env-var → vault-key *references*
//! — it stores key NAMES, never secret values, so profiles are safe to persist and
//! serialize. The actual values live only in the OS keychain, reached through the
//! platform secrets capability (CPE-268), abstracted here as [`SecretAccess`] so this
//! logic is unit-testable and never touches a real store in tests.
//!
//! At launch, [`resolve_env`] turns a profile into the concrete env vars for a session
//! (feeding the routing engine, CPE-285). This is the "logins to different envs
//! without compromising security" requirement.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The vault's schema version (CPE-300 discipline).
pub const VAULT_SCHEMA_VERSION: u16 = 1;

/// Brokered access to the secret store. In production this issues `secrets.*` requests
/// to the host (CPE-268); tests use an in-memory fake. Secret VALUES flow only through
/// here — never into a profile, log, or the host UI.
pub trait SecretAccess {
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// A named credential profile: a mapping of ENV-VAR name → the vault key holding its
/// value. Contains **no secret values** — only references.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialProfile {
    pub name: String,
    /// `ENV_VAR_NAME` → vault secret key.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// The persisted set of profiles (stored via the sidecar's storage capability).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSet {
    pub schema_version: u16,
    #[serde(default)]
    pub profiles: BTreeMap<String, CredentialProfile>,
}

impl Default for ProfileSet {
    fn default() -> Self {
        Self { schema_version: VAULT_SCHEMA_VERSION, profiles: BTreeMap::new() }
    }
}

impl ProfileSet {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, profile: CredentialProfile) {
        self.profiles.insert(profile.name.clone(), profile);
    }
    pub fn get(&self, name: &str) -> Option<&CredentialProfile> {
        self.profiles.get(name)
    }
    pub fn remove(&mut self, name: &str) -> Option<CredentialProfile> {
        self.profiles.remove(name)
    }
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.profiles.keys()
    }
}

/// Store a secret value under `key` (e.g. an API key just entered by the user).
pub fn store_secret(access: &dyn SecretAccess, key: &str, value: &str) -> Result<(), String> {
    access.set(key, value)
}

/// Resolve a profile to the concrete env vars for a launch, fetching each referenced
/// secret from the vault. Errors if any referenced secret is missing — so a session
/// never launches with a half-populated environment.
pub fn resolve_env(
    profile: &CredentialProfile,
    access: &dyn SecretAccess,
) -> Result<BTreeMap<String, String>, String> {
    let mut env = BTreeMap::new();
    for (var, key) in &profile.env {
        let value = access
            .get(key)?
            .ok_or_else(|| format!("secret '{key}' for env '{var}' is not set"))?;
        env.insert(var.clone(), value);
    }
    Ok(env)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemAccess {
        map: Mutex<HashMap<String, String>>,
    }
    impl SecretAccess for MemAccess {
        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.map.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            Ok(self.map.lock().unwrap().get(key).cloned())
        }
        fn delete(&self, key: &str) -> Result<(), String> {
            self.map.lock().unwrap().remove(key);
            Ok(())
        }
    }

    fn work_profile() -> CredentialProfile {
        CredentialProfile {
            name: "work".into(),
            env: BTreeMap::from([("OPENROUTER_API_KEY".into(), "openrouter.work".into())]),
        }
    }

    #[test]
    fn resolve_env_fills_values_from_the_vault() {
        let access = MemAccess::default();
        store_secret(&access, "openrouter.work", "sk-or-xyz").unwrap();
        let env = resolve_env(&work_profile(), &access).unwrap();
        assert_eq!(env["OPENROUTER_API_KEY"], "sk-or-xyz");
    }

    #[test]
    fn resolve_env_errors_when_a_secret_is_missing() {
        let access = MemAccess::default();
        let err = resolve_env(&work_profile(), &access).unwrap_err();
        assert!(err.contains("not set"));
    }

    #[test]
    fn a_profile_stores_references_not_values() {
        // The design guarantee: serializing a profile never leaks a secret value.
        let json = serde_json::to_string(&work_profile()).unwrap();
        assert!(json.contains("openrouter.work")); // the key reference
        assert!(!json.contains("sk-or")); // never a value
    }

    #[test]
    fn profile_set_add_get_remove_and_persist() {
        let mut set = ProfileSet::new();
        set.add(work_profile());
        set.add(CredentialProfile { name: "personal".into(), env: BTreeMap::new() });
        assert_eq!(set.names().count(), 2);
        assert!(set.get("work").is_some());
        // Round-trips through JSON (as stored via the storage capability).
        let json = serde_json::to_string(&set).unwrap();
        let back: ProfileSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_version, VAULT_SCHEMA_VERSION);
        assert_eq!(back.profiles.len(), 2);
        set.remove("personal");
        assert_eq!(set.names().count(), 1);
    }

    #[test]
    fn switching_profiles_yields_different_environments() {
        let access = MemAccess::default();
        store_secret(&access, "openrouter.work", "sk-work").unwrap();
        store_secret(&access, "openrouter.home", "sk-home").unwrap();
        let home = CredentialProfile {
            name: "home".into(),
            env: BTreeMap::from([("OPENROUTER_API_KEY".into(), "openrouter.home".into())]),
        };
        assert_eq!(resolve_env(&work_profile(), &access).unwrap()["OPENROUTER_API_KEY"], "sk-work");
        assert_eq!(resolve_env(&home, &access).unwrap()["OPENROUTER_API_KEY"], "sk-home");
    }
}
