//! Agent registry + agent manifest schema (CPE-278).
//!
//! The heart of "CLI-agnostic and extensible": each coding-agent CLI is described by a
//! declarative `agent.json` manifest — how to detect / install / update / uninstall /
//! run it per OS, which providers it supports, and its default model. The
//! [`AgentRegistry`] loads bundled + user manifests so adding an agent is **data, not
//! code**. Modelled on the per-agent script folders in the `AgenticCliOptions` project,
//! ported to a manifest.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The agent-manifest schema version this build understands (CPE-300 discipline).
pub const AGENT_SCHEMA_VERSION: u16 = 1;

/// A command to run for one lifecycle step on one OS. Rust orchestrates the underlying
/// package manager / CLI — there are no shell scripts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsCommand {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// How to launch an agent against one provider: environment variables and extra run
/// args, as templates with `{model}`, `{small_model}`, `{api_key}`, `{base_url}`
/// placeholders (CPE-285). Declaring these in the manifest keeps routing CLI-agnostic —
/// a different agent uses different env-var names, all as data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRecipe {
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub args: Vec<String>,
}

/// A declarative description of a coding-agent CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentManifest {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    /// Reports whether/what version is installed (e.g. `claude --version`), per OS.
    #[serde(default)]
    pub detect: BTreeMap<String, OsCommand>,
    #[serde(default)]
    pub install: BTreeMap<String, OsCommand>,
    #[serde(default)]
    pub update: BTreeMap<String, OsCommand>,
    #[serde(default)]
    pub uninstall: BTreeMap<String, OsCommand>,
    /// How to launch the agent, per OS.
    #[serde(default)]
    pub run: BTreeMap<String, OsCommand>,
    /// Providers this agent supports, e.g. `["native", "openrouter", "lmstudio-local"]`.
    #[serde(default)]
    pub providers: Vec<String>,
    /// Per-provider launch recipes (env + args templates), keyed by provider id (CPE-285).
    #[serde(default)]
    pub provider_recipes: BTreeMap<String, ProviderRecipe>,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(skip)]
    pub source_dir: PathBuf,
}

impl AgentManifest {
    pub fn current_os_key() -> &'static str {
        if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        }
    }

    pub fn detect_for_current_os(&self) -> Option<&OsCommand> {
        self.detect.get(Self::current_os_key())
    }
    pub fn install_for_current_os(&self) -> Option<&OsCommand> {
        self.install.get(Self::current_os_key())
    }
    pub fn update_for_current_os(&self) -> Option<&OsCommand> {
        self.update.get(Self::current_os_key())
    }
    pub fn uninstall_for_current_os(&self) -> Option<&OsCommand> {
        self.uninstall.get(Self::current_os_key())
    }
    pub fn run_for_current_os(&self) -> Option<&OsCommand> {
        self.run.get(Self::current_os_key())
    }

    pub fn supports_provider(&self, provider: &str) -> bool {
        self.providers.iter().any(|p| p == provider)
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version == 0 || self.schema_version > AGENT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported agent schema_version {} (this build supports up to {})",
                self.schema_version, AGENT_SCHEMA_VERSION
            ));
        }
        if self.id.trim().is_empty() {
            return Err("agent manifest has an empty id".into());
        }
        if self.run.is_empty() {
            return Err("agent manifest declares no run command".into());
        }
        Ok(())
    }
}

/// A manifest that was skipped during loading, with why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadWarning {
    pub path: PathBuf,
    pub reason: String,
}

/// The loaded, validated set of agent manifests, keyed by id.
#[derive(Debug, Default)]
pub struct AgentRegistry {
    by_id: BTreeMap<String, AgentManifest>,
    warnings: Vec<LoadWarning>,
}

impl AgentRegistry {
    /// Scan the dirs in order; later dirs override earlier by id (user over bundled).
    /// Malformed / unknown-future-schema / invalid manifests are skipped with a
    /// recorded reason, never fatal.
    pub fn load_from_dirs(dirs: &[PathBuf]) -> AgentRegistry {
        let mut reg = AgentRegistry::default();
        for dir in dirs {
            reg.scan_dir(dir);
        }
        reg
    }

    fn scan_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        files.sort();
        for path in files {
            self.load_file(&path);
        }
    }

    fn load_file(&mut self, path: &Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => return self.warn(path, format!("could not read: {e}")),
        };
        let mut manifest: AgentManifest = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => return self.warn(path, format!("invalid JSON/shape: {e}")),
        };
        if let Err(reason) = manifest.validate() {
            return self.warn(path, reason);
        }
        manifest.source_dir = path.parent().unwrap_or(path).to_path_buf();
        self.by_id.insert(manifest.id.clone(), manifest);
    }

    fn warn(&mut self, path: &Path, reason: String) {
        self.warnings.push(LoadWarning { path: path.to_path_buf(), reason });
    }

    pub fn get(&self, id: &str) -> Option<&AgentManifest> {
        self.by_id.get(id)
    }
    pub fn all(&self) -> impl Iterator<Item = &AgentManifest> {
        self.by_id.values()
    }
    pub fn len(&self) -> usize {
        self.by_id.len()
    }
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
    pub fn warnings(&self) -> &[LoadWarning] {
        &self.warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, json: &str) {
        let mut f = std::fs::File::create(dir.join(name)).unwrap();
        f.write_all(json.as_bytes()).unwrap();
    }

    fn claude_manifest() -> &'static str {
        r#"{
          "schema_version": 1,
          "id": "claude",
          "name": "Claude Code",
          "detect": {
            "windows": { "command": "claude", "args": ["--version"] },
            "macos": { "command": "claude", "args": ["--version"] },
            "linux": { "command": "claude", "args": ["--version"] }
          },
          "install": {
            "windows": { "command": "npm", "args": ["install", "-g", "@anthropic-ai/claude-code@latest"] },
            "macos": { "command": "npm", "args": ["install", "-g", "@anthropic-ai/claude-code@latest"] },
            "linux": { "command": "npm", "args": ["install", "-g", "@anthropic-ai/claude-code@latest"] }
          },
          "run": {
            "windows": { "command": "claude" },
            "macos": { "command": "claude" },
            "linux": { "command": "claude" }
          },
          "providers": ["native", "openrouter", "lmstudio-local"],
          "default_model": "claude-sonnet-4-5"
        }"#
    }

    #[test]
    fn loads_and_resolves_an_agent_manifest() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "claude.json", claude_manifest());
        let reg = AgentRegistry::load_from_dirs(&[d.path().to_path_buf()]);
        assert_eq!(reg.len(), 1);
        let m = reg.get("claude").unwrap();
        assert_eq!(m.name, "Claude Code");
        assert!(m.detect_for_current_os().is_some());
        assert!(m.run_for_current_os().is_some());
        assert!(m.supports_provider("openrouter"));
        assert!(!m.supports_provider("bedrock"));
        assert_eq!(m.default_model.as_deref(), Some("claude-sonnet-4-5"));
        assert!(reg.warnings().is_empty());
    }

    #[test]
    fn skips_malformed_without_failing_the_scan() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "good.json", claude_manifest());
        write(d.path(), "bad.json", "{ not json ");
        let reg = AgentRegistry::load_from_dirs(&[d.path().to_path_buf()]);
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.warnings().len(), 1);
    }

    #[test]
    fn skips_a_manifest_with_no_run_command() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "norun.json",
            r#"{ "schema_version": 1, "id": "x", "name": "X" }"#,
        );
        let reg = AgentRegistry::load_from_dirs(&[d.path().to_path_buf()]);
        assert!(reg.is_empty());
        assert!(reg.warnings()[0].reason.contains("no run command"));
    }

    #[test]
    fn skips_unknown_future_schema() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            "future.json",
            r#"{ "schema_version": 99, "id": "x", "name": "X", "run": { "linux": { "command": "x" }, "windows": { "command": "x" }, "macos": { "command": "x" } } }"#,
        );
        let reg = AgentRegistry::load_from_dirs(&[d.path().to_path_buf()]);
        assert!(reg.is_empty());
        assert!(reg.warnings()[0].reason.contains("schema_version"));
    }

    #[test]
    fn user_dir_overrides_bundled_by_id() {
        let bundled = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        write(bundled.path(), "claude.json", claude_manifest());
        // User overrides the default model.
        let overridden = claude_manifest().replace("claude-sonnet-4-5", "claude-opus-4-8");
        write(user.path(), "claude.json", &overridden);
        let reg = AgentRegistry::load_from_dirs(&[
            bundled.path().to_path_buf(),
            user.path().to_path_buf(),
        ]);
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get("claude").unwrap().default_model.as_deref(), Some("claude-opus-4-8"));
    }
}
