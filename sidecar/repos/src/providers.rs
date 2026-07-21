//! Provider registry + provider-manifest schema (CPE-430).
//!
//! Each forge / VCS the sidecar can talk to is described by a declarative `*.json` manifest — how to
//! identify it, which VCS backend drives it, how you authenticate, what it can do, and which API
//! host(s) it needs (so the host can build an **allow-listed** egress set, CPE-433). The
//! [`ProviderRegistry`] loads bundled + user manifests so adding a provider is **data, not code**.
//! Modelled on the Agent Deck's agent registry (CPE-278).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The provider-manifest schema version this build understands (CPE-300 discipline).
pub const PROVIDER_SCHEMA_VERSION: u16 = 1;

/// A declarative description of a forge / VCS provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderManifest {
    pub schema_version: u16,
    /// Stable id, e.g. `"github"`.
    pub id: String,
    /// Display name, e.g. `"GitHub"`.
    pub name: String,
    /// The VCS backend that drives it: `"git"`, `"hg"`, `"svn"`, `"perforce"`, `"fossil"`.
    pub kind: String,
    /// Auth models this provider supports: any of `"oauth"`, `"pat"`, `"ssh"`, `"anonymous"`.
    #[serde(default)]
    pub auth: Vec<String>,
    /// API host(s) the sidecar's operations need — the host unions these into its egress allow-list
    /// (CPE-433); the sidecar itself never opens a socket. Empty = no host-brokered API (generic Git).
    #[serde(default)]
    pub api_hosts: Vec<String>,
    /// Capabilities this provider offers: any of `"browse"`, `"clone"`, `"pull"`, `"push"`.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// The provider's web base URL (for "open in browser"), e.g. `"https://github.com"`.
    #[serde(default)]
    pub web_base: Option<String>,
    /// Not production-ready (Tier 2/3 VCS): shown but flagged.
    #[serde(default)]
    pub experimental: bool,
    #[serde(skip)]
    pub source_dir: PathBuf,
}

/// The VCS backends this build recognises. An unknown `kind` is refused so a typo can't silently ship.
const KNOWN_KINDS: &[&str] = &["git", "hg", "svn", "perforce", "fossil"];

impl ProviderManifest {
    pub fn supports_capability(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c == cap)
    }
    pub fn supports_auth(&self, auth: &str) -> bool {
        self.auth.iter().any(|a| a == auth)
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version == 0 || self.schema_version > PROVIDER_SCHEMA_VERSION {
            return Err(format!(
                "unsupported provider schema_version {} (this build supports up to {})",
                self.schema_version, PROVIDER_SCHEMA_VERSION
            ));
        }
        if self.id.trim().is_empty() {
            return Err("provider manifest has an empty id".into());
        }
        if self.name.trim().is_empty() {
            return Err("provider manifest has an empty name".into());
        }
        if !KNOWN_KINDS.contains(&self.kind.as_str()) {
            return Err(format!("provider '{}' has unknown kind '{}'", self.id, self.kind));
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

/// The loaded, validated set of provider manifests, keyed by id.
#[derive(Debug, Default)]
pub struct ProviderRegistry {
    by_id: BTreeMap<String, ProviderManifest>,
    warnings: Vec<LoadWarning>,
}

impl ProviderRegistry {
    /// Scan the dirs in order; later dirs override earlier by id (user over bundled). Malformed /
    /// unknown-future-schema / invalid manifests are skipped with a recorded reason, never fatal.
    pub fn load_from_dirs(dirs: &[PathBuf]) -> ProviderRegistry {
        let mut reg = ProviderRegistry::default();
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
        match serde_json::from_str::<ProviderManifest>(&text)
            .map_err(|e| format!("invalid JSON/shape: {e}"))
            .and_then(|m| m.validate().map(|_| m))
        {
            Ok(mut manifest) => {
                manifest.source_dir = path.parent().unwrap_or(path).to_path_buf();
                self.by_id.insert(manifest.id.clone(), manifest);
            }
            Err(reason) => self.warn(path, reason),
        }
    }

    fn warn(&mut self, path: &Path, reason: String) {
        self.warnings.push(LoadWarning { path: path.to_path_buf(), reason });
    }

    pub fn get(&self, id: &str) -> Option<&ProviderManifest> {
        self.by_id.get(id)
    }
    pub fn all(&self) -> impl Iterator<Item = &ProviderManifest> {
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

    /// The union of every provider's `api_hosts` — the host's egress allow-list (CPE-433). Sorted +
    /// de-duplicated.
    pub fn egress_allow_list(&self) -> Vec<String> {
        let mut hosts: Vec<String> =
            self.by_id.values().flat_map(|p| p.api_hosts.iter().cloned()).collect();
        hosts.sort();
        hosts.dedup();
        hosts
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

    fn github() -> &'static str {
        r#"{
          "schema_version": 1, "id": "github", "name": "GitHub", "kind": "git",
          "auth": ["oauth", "pat", "ssh"], "api_hosts": ["api.github.com"],
          "capabilities": ["browse", "clone", "pull", "push"], "web_base": "https://github.com"
        }"#
    }

    #[test]
    fn loads_and_resolves_a_provider_manifest() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "github.json", github());
        let reg = ProviderRegistry::load_from_dirs(&[d.path().to_path_buf()]);
        assert_eq!(reg.len(), 1);
        let p = reg.get("github").unwrap();
        assert_eq!(p.name, "GitHub");
        assert_eq!(p.kind, "git");
        assert!(p.supports_capability("clone"));
        assert!(!p.supports_capability("issues"));
        assert!(p.supports_auth("pat"));
        assert_eq!(reg.egress_allow_list(), vec!["api.github.com".to_string()]);
        assert!(reg.warnings().is_empty());
    }

    #[test]
    fn skips_malformed_unknown_kind_and_future_schema_without_failing_the_scan() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "good.json", github());
        write(d.path(), "bad.json", "{ not json ");
        write(
            d.path(),
            "weird.json",
            r#"{ "schema_version": 1, "id": "weird", "name": "W", "kind": "cvs" }"#,
        );
        write(
            d.path(),
            "future.json",
            r#"{ "schema_version": 99, "id": "f", "name": "F", "kind": "git" }"#,
        );
        let reg = ProviderRegistry::load_from_dirs(&[d.path().to_path_buf()]);
        assert_eq!(reg.len(), 1, "only the good manifest loads");
        assert!(reg.get("github").is_some());
        assert_eq!(reg.warnings().len(), 3);
    }

    #[test]
    fn user_dir_overrides_bundled_by_id() {
        let bundled = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        write(bundled.path(), "github.json", github());
        write(user.path(), "github.json", &github().replace("GitHub", "GitHub Enterprise"));
        let reg = ProviderRegistry::load_from_dirs(&[
            bundled.path().to_path_buf(),
            user.path().to_path_buf(),
        ]);
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get("github").unwrap().name, "GitHub Enterprise");
    }

    #[test]
    fn egress_allow_list_unions_and_dedupes_hosts() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "github.json", github());
        write(
            d.path(),
            "gitlab.json",
            r#"{ "schema_version": 1, "id": "gitlab", "name": "GitLab", "kind": "git",
                 "api_hosts": ["gitlab.com", "api.github.com"], "capabilities": ["browse"] }"#,
        );
        let reg = ProviderRegistry::load_from_dirs(&[d.path().to_path_buf()]);
        assert_eq!(reg.egress_allow_list(), vec!["api.github.com".to_string(), "gitlab.com".to_string()]);
    }
}
