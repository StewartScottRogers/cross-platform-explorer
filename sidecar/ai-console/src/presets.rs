//! Launcher presets & remembered selection (CPE-352/353).
//!
//! Persists the launcher's upper-panel selection so it comes back between usages, and stores
//! named "sets" per coding agent. **No secret VALUES are ever kept here** — `key_ref` merely
//! names which stored provider credential to use (the value lives in the OS keychain, CPE-287),
//! so remembering a selection keeps the API key by reference, not in plaintext.
//!
//! The pure model below is unit-tested; a `PresetsBackend` persists it to the host storage
//! directory (CPE-268) via the broker client, with an in-memory fallback for dev/tests.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const PRESETS_SCHEMA_VERSION: u16 = 1;

/// One saved launcher configuration for an agent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    /// Display name of the set. Empty for the implicit "last used" entry.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub small_model: String,
    /// Which stored provider credential to use — a reference, never the key value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_ref: Option<String>,
}

/// An agent's named sets plus its last-used selection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPresets {
    #[serde(default)]
    pub presets: Vec<Preset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<Preset>,
}

/// A stored credential's identity — which labelled key a provider has (CPE-348). Values
/// live in the OS keychain; this index just tracks the NAMES so labelled keys are listable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialRef {
    pub provider: String,
    pub label: String,
}

/// The whole persisted document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetStore {
    pub schema_version: u16,
    /// The agent selected on the most recent launch, restored on open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent: Option<String>,
    /// Whether the user has dismissed the first-run onboarding (CPE-312).
    #[serde(default)]
    pub onboarded: bool,
    /// Which labelled provider credentials exist — names only (CPE-348).
    #[serde(default)]
    pub credentials: Vec<CredentialRef>,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentPresets>,
    /// Auto-refresh the agent catalog on open (CPE-378). Opt-in; default off.
    #[serde(default)]
    pub auto_update_catalog: bool,
    /// Agent ids the user has pinned — catalog updates skip these (CPE-378).
    #[serde(default)]
    pub pinned_agents: Vec<String>,
}

impl Default for PresetStore {
    fn default() -> Self {
        Self {
            schema_version: PRESETS_SCHEMA_VERSION,
            last_agent: None,
            onboarded: false,
            credentials: Vec::new(),
            agents: BTreeMap::new(),
            auto_update_catalog: false,
            pinned_agents: Vec::new(),
        }
    }
}

impl PresetStore {
    /// Record the selection just launched as `agent`'s last-used (and the global last agent).
    pub fn remember(&mut self, agent: &str, mut selection: Preset) {
        selection.name = String::new(); // last-used is nameless
        self.agents.entry(agent.to_string()).or_default().last_used = Some(selection);
        self.last_agent = Some(agent.to_string());
    }

    pub fn last_used(&self, agent: &str) -> Option<&Preset> {
        self.agents.get(agent).and_then(|a| a.last_used.as_ref())
    }

    pub fn presets(&self, agent: &str) -> &[Preset] {
        self.agents.get(agent).map(|a| a.presets.as_slice()).unwrap_or(&[])
    }

    /// Pin (or unpin) an agent so catalog updates skip it (CPE-378).
    pub fn set_pinned(&mut self, agent: &str, pinned: bool) {
        self.pinned_agents.retain(|a| a != agent);
        if pinned {
            self.pinned_agents.push(agent.to_string());
        }
    }

    pub fn is_pinned(&self, agent: &str) -> bool {
        self.pinned_agents.iter().any(|a| a == agent)
    }

    /// Save (or update by name) a named set for an agent.
    pub fn save_preset(&mut self, agent: &str, preset: Preset) {
        let a = self.agents.entry(agent.to_string()).or_default();
        match a.presets.iter_mut().find(|p| p.name == preset.name) {
            Some(existing) => *existing = preset,
            None => a.presets.push(preset),
        }
    }

    pub fn delete_preset(&mut self, agent: &str, name: &str) {
        if let Some(a) = self.agents.get_mut(agent) {
            a.presets.retain(|p| p.name != name);
        }
    }

    /// Record that a labelled credential exists for a provider (CPE-348, dedup).
    pub fn add_credential(&mut self, provider: &str, label: &str) {
        if !self.credentials.iter().any(|c| c.provider == provider && c.label == label) {
            self.credentials.push(CredentialRef { provider: provider.into(), label: label.into() });
        }
    }

    pub fn remove_credential(&mut self, provider: &str, label: &str) {
        self.credentials.retain(|c| !(c.provider == provider && c.label == label));
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Parse, tolerating a corrupt/hand-edited file by falling back to an empty store.
    pub fn from_json(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }
}

/// Persistence for the preset store. Implementations must be `Send + Sync` (shared across the
/// console's HTTP handler threads).
pub trait PresetsBackend: Send + Sync {
    /// Load the store, returning a default on any error (never fails the console).
    fn load(&self) -> PresetStore;
    /// Persist the store.
    fn save(&self, store: &PresetStore) -> Result<(), String>;
}

/// In-memory backend for dev/standalone runs (no host storage). Not durable.
#[derive(Default)]
pub struct MemPresets {
    store: std::sync::Mutex<PresetStore>,
}

impl PresetsBackend for MemPresets {
    fn load(&self) -> PresetStore {
        self.store.lock().unwrap().clone()
    }
    fn save(&self, store: &PresetStore) -> Result<(), String> {
        *self.store.lock().unwrap() = store.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(provider: &str, model: &str) -> Preset {
        Preset { name: String::new(), provider: provider.into(), model: model.into(), ..Default::default() }
    }

    #[test]
    fn remember_sets_per_agent_last_used_and_global_last_agent() {
        let mut s = PresetStore::default();
        s.remember("claude", sel("openrouter", "sonnet"));
        s.remember("codex", sel("openai", "gpt"));
        assert_eq!(s.last_agent.as_deref(), Some("codex"));
        assert_eq!(s.last_used("claude").unwrap().provider, "openrouter");
        assert_eq!(s.last_used("claude").unwrap().model, "sonnet");
        assert_eq!(s.last_used("codex").unwrap().provider, "openai");
        assert!(s.last_used("gemini").is_none());
    }

    #[test]
    fn remember_strips_any_name_so_last_used_is_nameless() {
        let mut s = PresetStore::default();
        s.remember("claude", Preset { name: "should-drop".into(), provider: "x".into(), ..Default::default() });
        assert_eq!(s.last_used("claude").unwrap().name, "");
    }

    #[test]
    fn save_preset_adds_then_updates_by_name() {
        let mut s = PresetStore::default();
        s.save_preset("claude", Preset { name: "Work".into(), provider: "openrouter".into(), ..Default::default() });
        s.save_preset("claude", Preset { name: "Local".into(), provider: "lmstudio-local".into(), ..Default::default() });
        assert_eq!(s.presets("claude").len(), 2);
        // Same name updates in place.
        s.save_preset("claude", Preset { name: "Work".into(), provider: "openrouter".into(), model: "opus".into(), ..Default::default() });
        assert_eq!(s.presets("claude").len(), 2);
        let work = s.presets("claude").iter().find(|p| p.name == "Work").unwrap();
        assert_eq!(work.model, "opus");
    }

    #[test]
    fn delete_preset_removes_only_the_named_one() {
        let mut s = PresetStore::default();
        s.save_preset("claude", Preset { name: "Work".into(), ..Default::default() });
        s.save_preset("claude", Preset { name: "Local".into(), ..Default::default() });
        s.delete_preset("claude", "Work");
        assert_eq!(s.presets("claude").len(), 1);
        assert_eq!(s.presets("claude")[0].name, "Local");
    }

    #[test]
    fn json_round_trips_and_never_contains_a_key_value() {
        let mut s = PresetStore::default();
        s.save_preset("claude", Preset {
            name: "Work".into(),
            provider: "openrouter".into(),
            model: "sonnet".into(),
            small_model: "haiku".into(),
            key_ref: Some("provider:openrouter".into()),
        });
        let json = s.to_json();
        assert!(json.contains("provider:openrouter")); // the reference is fine
        assert!(!json.contains("sk-")); // no secret value ever
        let back = PresetStore::from_json(&json);
        assert_eq!(back, s);
    }

    #[test]
    fn from_json_tolerates_garbage() {
        assert_eq!(PresetStore::from_json("not json"), PresetStore::default());
        assert_eq!(PresetStore::from_json("{}").schema_version, PRESETS_SCHEMA_VERSION);
    }

    #[test]
    fn mem_backend_round_trips() {
        let b = MemPresets::default();
        let mut s = b.load();
        s.remember("claude", sel("openrouter", "sonnet"));
        b.save(&s).unwrap();
        assert_eq!(b.load().last_used("claude").unwrap().provider, "openrouter");
    }
}
