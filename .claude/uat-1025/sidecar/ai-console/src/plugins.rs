//! Plugin / extension system — MCP fan-out (CPE-288).
//!
//! Plugins (e.g. an MCP memory server, context7) extend agents with MCP servers, slash
//! commands, and tooling. A [`PluginManifest`] declares which agents it `supports`;
//! installing or uninstalling a plugin **fans across every supporting agent that is
//! installed**. The per-agent config edit is abstracted behind [`PluginApplier`] so the
//! fan-out logic is unit-testable; production appliers edit each agent's MCP config.
//! Ported from the `Plugins/` system of the reference project.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The plugin-manifest schema version (CPE-300 discipline).
pub const PLUGIN_SCHEMA_VERSION: u16 = 1;

/// A declarative plugin description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    /// e.g. `"mcp-server"`.
    pub kind: String,
    #[serde(default)]
    pub description: String,
    /// Agent ids this plugin can extend.
    #[serde(default)]
    pub supports: Vec<String>,
    #[serde(skip)]
    pub source_dir: PathBuf,
}

impl PluginManifest {
    /// The subset of `installed_agents` that this plugin supports — the fan-out target.
    pub fn applicable_agents<'a>(&self, installed_agents: &'a [String]) -> Vec<&'a String> {
        installed_agents
            .iter()
            .filter(|a| self.supports.iter().any(|s| s == *a))
            .collect()
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version == 0 || self.schema_version > PLUGIN_SCHEMA_VERSION {
            return Err(format!("unsupported plugin schema_version {}", self.schema_version));
        }
        if self.id.trim().is_empty() {
            return Err("plugin manifest has an empty id".into());
        }
        Ok(())
    }
}

/// Applies (or removes) a plugin's effect on ONE agent — e.g. adding/removing its MCP
/// server entry in that agent's config. Idempotent: applying twice is a no-op.
pub trait PluginApplier {
    fn apply(&self, plugin: &PluginManifest, agent_id: &str) -> Result<(), String>;
    fn remove(&self, plugin: &PluginManifest, agent_id: &str) -> Result<(), String>;
}

/// The outcome of fanning a plugin action across one agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanOutcome {
    pub agent_id: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// Install a plugin across every supporting installed agent. Continues past failures.
pub fn install_plugin(
    plugin: &PluginManifest,
    installed_agents: &[String],
    applier: &dyn PluginApplier,
) -> Vec<FanOutcome> {
    fan(plugin, installed_agents, applier, true)
}

/// Uninstall a plugin from every supporting installed agent.
pub fn uninstall_plugin(
    plugin: &PluginManifest,
    installed_agents: &[String],
    applier: &dyn PluginApplier,
) -> Vec<FanOutcome> {
    fan(plugin, installed_agents, applier, false)
}

fn fan(
    plugin: &PluginManifest,
    installed_agents: &[String],
    applier: &dyn PluginApplier,
    install: bool,
) -> Vec<FanOutcome> {
    plugin
        .applicable_agents(installed_agents)
        .into_iter()
        .map(|agent_id| {
            let result = if install {
                applier.apply(plugin, agent_id)
            } else {
                applier.remove(plugin, agent_id)
            };
            match result {
                Ok(()) => FanOutcome { agent_id: agent_id.clone(), ok: true, error: None },
                Err(e) => FanOutcome { agent_id: agent_id.clone(), ok: false, error: Some(e) },
            }
        })
        .collect()
}

/// The loaded, validated set of plugin manifests, keyed by id (bundled + user dirs).
#[derive(Debug, Default)]
pub struct PluginRegistry {
    by_id: BTreeMap<String, PluginManifest>,
    warnings: Vec<(PathBuf, String)>,
}

impl PluginRegistry {
    pub fn load_from_dirs(dirs: &[PathBuf]) -> PluginRegistry {
        let mut reg = PluginRegistry::default();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            let mut files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
                .collect();
            files.sort();
            for path in files {
                reg.load_file(&path);
            }
        }
        reg
    }

    fn load_file(&mut self, path: &Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => return self.warnings.push((path.into(), format!("read: {e}"))),
        };
        let mut m: PluginManifest = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => return self.warnings.push((path.into(), format!("invalid: {e}"))),
        };
        if let Err(reason) = m.validate() {
            return self.warnings.push((path.into(), reason));
        }
        m.source_dir = path.parent().unwrap_or(path).to_path_buf();
        self.by_id.insert(m.id.clone(), m);
    }

    pub fn get(&self, id: &str) -> Option<&PluginManifest> {
        self.by_id.get(id)
    }
    pub fn all(&self) -> impl Iterator<Item = &PluginManifest> {
        self.by_id.values()
    }
    pub fn len(&self) -> usize {
        self.by_id.len()
    }
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
    pub fn warnings(&self) -> &[(PathBuf, String)] {
        &self.warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn cipher() -> PluginManifest {
        PluginManifest {
            schema_version: 1,
            id: "cipher-mcp".into(),
            name: "Cipher".into(),
            kind: "mcp-server".into(),
            description: "memory".into(),
            supports: vec!["claude".into(), "codex".into(), "gemini".into()],
            source_dir: PathBuf::new(),
        }
    }

    #[test]
    fn applicable_agents_is_supports_intersect_installed() {
        let installed = vec!["claude".to_string(), "aider".to_string(), "codex".to_string()];
        let applicable = cipher().applicable_agents(&installed);
        assert_eq!(applicable, vec![&"claude".to_string(), &"codex".to_string()]);
    }

    /// Records what was applied/removed.
    #[derive(Default)]
    struct RecordingApplier {
        applied: RefCell<Vec<String>>,
        removed: RefCell<Vec<String>>,
        fail_on: Option<String>,
    }
    impl PluginApplier for RecordingApplier {
        fn apply(&self, _p: &PluginManifest, agent_id: &str) -> Result<(), String> {
            if self.fail_on.as_deref() == Some(agent_id) {
                return Err("boom".into());
            }
            self.applied.borrow_mut().push(agent_id.to_string());
            Ok(())
        }
        fn remove(&self, _p: &PluginManifest, agent_id: &str) -> Result<(), String> {
            self.removed.borrow_mut().push(agent_id.to_string());
            Ok(())
        }
    }

    #[test]
    fn install_fans_across_supporting_installed_agents() {
        let installed = vec!["claude".to_string(), "codex".to_string(), "aider".to_string()];
        let applier = RecordingApplier::default();
        let outcomes = install_plugin(&cipher(), &installed, &applier);
        assert_eq!(outcomes.len(), 2); // claude + codex (not aider)
        assert!(outcomes.iter().all(|o| o.ok));
        assert_eq!(*applier.applied.borrow(), vec!["claude", "codex"]);
    }

    #[test]
    fn fan_out_continues_past_a_failing_agent() {
        let installed = vec!["claude".to_string(), "codex".to_string()];
        let applier = RecordingApplier { fail_on: Some("claude".into()), ..Default::default() };
        let outcomes = install_plugin(&cipher(), &installed, &applier);
        assert_eq!(outcomes.len(), 2);
        assert!(!outcomes[0].ok && outcomes[0].agent_id == "claude");
        assert!(outcomes[1].ok && outcomes[1].agent_id == "codex");
    }

    #[test]
    fn uninstall_removes_from_supporting_agents() {
        let installed = vec!["claude".to_string(), "codex".to_string()];
        let applier = RecordingApplier::default();
        uninstall_plugin(&cipher(), &installed, &applier);
        assert_eq!(*applier.removed.borrow(), vec!["claude", "codex"]);
    }

    #[test]
    fn registry_loads_and_skips_bad() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("cipher.json"),
            serde_json::to_string(&cipher()).unwrap(),
        )
        .unwrap();
        std::fs::write(d.path().join("bad.json"), "{ nope").unwrap();
        let reg = PluginRegistry::load_from_dirs(&[d.path().to_path_buf()]);
        assert_eq!(reg.len(), 1);
        assert!(reg.get("cipher-mcp").is_some());
        assert_eq!(reg.warnings().len(), 1);
    }
}
