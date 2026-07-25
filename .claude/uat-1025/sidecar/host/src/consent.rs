//! Persisted capability consent per sidecar (CPE-296).
//!
//! "No ambient authority" needs a human in the loop: a sidecar *requests* capabilities
//! and the user grants or denies each one. This module persists those decisions so the
//! user is asked **once** per capability (not every launch), while a *newly requested*
//! capability after an update re-prompts for just that one. The granted set feeds
//! [`crate::broker::decide_grants`]; denial simply leaves the capability out, so the
//! sidecar degrades gracefully rather than crashing.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sidecar_contract::Capability;

const FILE: &str = "consent.json";
const SCHEMA_VERSION: u32 = 1;

/// One sidecar's recorded decisions. A capability in neither set is *undecided* and
/// triggers a prompt.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SidecarConsent {
    /// Capabilities the user explicitly granted.
    pub granted: BTreeSet<Capability>,
    /// Capabilities the user explicitly denied (remembered so we don't re-prompt).
    pub denied: BTreeSet<Capability>,
}

/// Persisted consent for all sidecars, stored as `consent.json` in the app config dir.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentStore {
    schema_version: u32,
    sidecars: BTreeMap<String, SidecarConsent>,
    #[serde(skip)]
    path: PathBuf,
}

impl Default for ConsentStore {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            sidecars: BTreeMap::new(),
            path: PathBuf::new(),
        }
    }
}

impl ConsentStore {
    /// Load the store from `dir/consent.json`, or start empty if absent/unreadable.
    /// Remembers `dir` so later mutations persist back to the same file.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(FILE);
        let mut store: ConsentStore = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        store.path = path;
        store
    }

    /// The granted (consented) set for a sidecar — empty if nothing is recorded.
    pub fn granted(&self, id: &str) -> BTreeSet<Capability> {
        self.sidecars.get(id).map(|c| c.granted.clone()).unwrap_or_default()
    }

    /// The full recorded decision for a sidecar, if any.
    pub fn get(&self, id: &str) -> Option<&SidecarConsent> {
        self.sidecars.get(id)
    }

    /// Which of `requested` are undecided (neither granted nor denied) and therefore need
    /// a consent prompt. Empty ⇒ nothing to ask; launch straight away.
    pub fn undecided(&self, id: &str, requested: &[Capability]) -> Vec<Capability> {
        let c = self.sidecars.get(id);
        requested
            .iter()
            .copied()
            .filter(|cap| c.is_none_or(|c| !c.granted.contains(cap) && !c.denied.contains(cap)))
            .collect()
    }

    /// Record a decision over `decided`: those in `granted` are approved, the rest denied.
    /// Idempotent and additive — prior decisions for other capabilities are preserved.
    pub fn record(
        &mut self,
        id: &str,
        granted: &BTreeSet<Capability>,
        decided: &[Capability],
    ) -> io::Result<()> {
        let entry = self.sidecars.entry(id.to_string()).or_default();
        for cap in decided {
            if granted.contains(cap) {
                entry.granted.insert(*cap);
                entry.denied.remove(cap);
            } else {
                entry.denied.insert(*cap);
                entry.granted.remove(cap);
            }
        }
        self.persist()
    }

    /// Revoke a previously-granted capability (management UI, CPE-274). It becomes denied,
    /// so the sidecar loses it on next launch without being re-prompted.
    pub fn revoke(&mut self, id: &str, cap: Capability) -> io::Result<()> {
        let entry = self.sidecars.entry(id.to_string()).or_default();
        entry.granted.remove(&cap);
        entry.denied.insert(cap);
        self.persist()
    }

    fn persist(&self) -> io::Result<()> {
        if self.path.as_os_str().is_empty() {
            return Ok(()); // no backing file (e.g. a default() used in-memory)
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&self.path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(caps: &[Capability]) -> BTreeSet<Capability> {
        caps.iter().copied().collect()
    }

    #[test]
    fn undecided_lists_only_unseen_capabilities() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = ConsentStore::load(dir.path());
        let requested = [Capability::Context, Capability::Secrets, Capability::Storage];
        // Nothing recorded yet ⇒ all three need a prompt.
        assert_eq!(s.undecided("ai-console", &requested), requested.to_vec());

        // Grant context, deny secrets; storage stays undecided.
        s.record("ai-console", &set(&[Capability::Context]), &[Capability::Context, Capability::Secrets])
            .unwrap();
        assert_eq!(s.undecided("ai-console", &requested), vec![Capability::Storage]);
    }

    #[test]
    fn denied_secrets_is_not_granted_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut s = ConsentStore::load(dir.path());
            s.record(
                "ai-console",
                &set(&[Capability::Context, Capability::Storage]),
                &[Capability::Context, Capability::Secrets, Capability::Storage],
            )
            .unwrap();
            assert!(!s.granted("ai-console").contains(&Capability::Secrets));
        }
        // Reload from disk: the decision survived.
        let s = ConsentStore::load(dir.path());
        assert_eq!(s.granted("ai-console"), set(&[Capability::Context, Capability::Storage]));
        assert!(s.get("ai-console").unwrap().denied.contains(&Capability::Secrets));
        // A denied capability is not re-prompted.
        assert!(s.undecided("ai-console", &[Capability::Secrets]).is_empty());
    }

    #[test]
    fn granted_feeds_decide_grants_to_exclude_denied() {
        use crate::broker::{decide_grants, GrantRequest};
        let dir = tempfile::tempdir().unwrap();
        let mut s = ConsentStore::load(dir.path());
        s.record(
            "ai-console",
            &set(&[Capability::Context]),
            &[Capability::Context, Capability::Secrets],
        )
        .unwrap();
        let granted = decide_grants(&GrantRequest {
            requested: vec![Capability::Context, Capability::Secrets],
            consented: s.granted("ai-console"),
            policy_allow: None,
        });
        // Denying secrets at consent means the broker never grants it.
        assert_eq!(granted, set(&[Capability::Context]));
    }

    #[test]
    fn revoke_removes_a_grant() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = ConsentStore::load(dir.path());
        s.record("ai-console", &set(&[Capability::Secrets]), &[Capability::Secrets]).unwrap();
        assert!(s.granted("ai-console").contains(&Capability::Secrets));
        s.revoke("ai-console", Capability::Secrets).unwrap();
        assert!(!s.granted("ai-console").contains(&Capability::Secrets));
        assert!(s.get("ai-console").unwrap().denied.contains(&Capability::Secrets));
    }
}
