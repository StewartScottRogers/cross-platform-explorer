//! Persisted enable/disable state per sidecar (CPE-274).
//!
//! Sidecars are enabled by default; the user can disable one from the management UI, which
//! stops it (if running) and prevents it from starting until re-enabled. Disabling is
//! per-sidecar and independent — it never touches the explorer or other sidecars. Stored
//! as `enablement.json` in the app config dir.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const FILE: &str = "enablement.json";

/// The set of *disabled* sidecar ids (absent ⇒ enabled, the default).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnablementStore {
    disabled: BTreeSet<String>,
    #[serde(skip)]
    path: PathBuf,
}

impl EnablementStore {
    /// Load from `dir/enablement.json`, or start empty (all enabled) if absent.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(FILE);
        let mut store: EnablementStore = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        store.path = path;
        store
    }

    /// Whether a sidecar is enabled (the default for anything not explicitly disabled).
    pub fn is_enabled(&self, id: &str) -> bool {
        !self.disabled.contains(id)
    }

    /// Enable or disable a sidecar and persist the change.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> io::Result<()> {
        if enabled {
            self.disabled.remove(id);
        } else {
            self.disabled.insert(id.to_string());
        }
        self.persist()
    }

    fn persist(&self) -> io::Result<()> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_by_default_then_toggles_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut s = EnablementStore::load(dir.path());
            assert!(s.is_enabled("ai-console"), "default is enabled");
            s.set_enabled("ai-console", false).unwrap();
            assert!(!s.is_enabled("ai-console"));
            // Other ids are unaffected — disabling is independent.
            assert!(s.is_enabled("some-other"));
        }
        // Reloads from disk.
        let mut s = EnablementStore::load(dir.path());
        assert!(!s.is_enabled("ai-console"), "disabled state survived");
        s.set_enabled("ai-console", true).unwrap();
        assert!(s.is_enabled("ai-console"));
    }
}
