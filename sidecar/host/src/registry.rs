//! Sidecar manifest schema + registry (CPE-264).
//!
//! A sidecar is described by a declarative `sidecar.json` manifest so that adding a
//! Mega-Feature is *data*, not code. The [`Registry`] scans a bundled directory and
//! a user-writable directory; a user manifest overrides a bundled one with the same
//! id. Manifests that are malformed, of an unknown future schema, or that target an
//! incompatible contract major are **skipped with a recorded reason**, never fatal.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sidecar_contract::{Capability, ContractVersion, CONTRACT_VERSION};

/// The manifest schema version this host understands. Bumped when the *manifest*
/// shape changes; unknown-future versions are refused, not mis-parsed (CPE-300).
pub const MANIFEST_SCHEMA_VERSION: u16 = 1;

/// How to launch a sidecar on one OS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryPoint {
    /// The executable to run (resolved relative to the manifest dir if not absolute).
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// How the host mounts the sidecar's own UI (CPE-271). `kind` is e.g. `"local_port"`
/// or `"bundle"`; `source` is the port/path/URL the host frames.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiMount {
    pub kind: String,
    pub source: String,
}

/// A declarative sidecar description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarManifest {
    pub schema_version: u16,
    pub id: String,
    pub name: String,
    pub version: String,
    /// The contract version the sidecar targets; must share the host's major.
    pub contract_version: ContractVersion,
    /// Per-OS entry points keyed by `"windows"` / `"macos"` / `"linux"`.
    pub entry: BTreeMap<String, EntryPoint>,
    /// Capabilities the sidecar requests (granted after consent, CPE-296).
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub ui: Option<UiMount>,
    /// Where this manifest was loaded from (not serialized; filled at load time).
    #[serde(skip)]
    pub source_dir: PathBuf,
}

impl SidecarManifest {
    /// The `entry` key for the OS we're compiled for.
    pub fn current_os_key() -> &'static str {
        if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "linux"
        }
    }

    /// The entry point for the current OS, if the manifest declares one.
    pub fn entry_for_current_os(&self) -> Option<&EntryPoint> {
        self.entry.get(Self::current_os_key())
    }

    /// Validate a freshly-parsed manifest against host expectations.
    fn validate(&self) -> Result<(), String> {
        if self.schema_version == 0 || self.schema_version > MANIFEST_SCHEMA_VERSION {
            return Err(format!(
                "unsupported manifest schema_version {} (host supports up to {})",
                self.schema_version, MANIFEST_SCHEMA_VERSION
            ));
        }
        if self.id.trim().is_empty() {
            return Err("manifest has an empty id".into());
        }
        if self.contract_version.major != CONTRACT_VERSION.major {
            return Err(format!(
                "manifest targets contract major {} but host speaks major {}",
                self.contract_version.major, CONTRACT_VERSION.major
            ));
        }
        if self.entry.is_empty() {
            return Err("manifest declares no entry points".into());
        }
        Ok(())
    }
}

/// A manifest that was skipped, with why — surfaced to diagnostics (CPE-298) rather
/// than failing the whole scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadWarning {
    pub path: PathBuf,
    pub reason: String,
}

/// The loaded, validated set of sidecar manifests, keyed by id.
#[derive(Debug, Default)]
pub struct Registry {
    by_id: BTreeMap<String, SidecarManifest>,
    warnings: Vec<LoadWarning>,
}

impl Registry {
    /// Scan the given directories in order; later dirs override earlier ones by id
    /// (so a user dir overrides bundled). Malformed/incompatible manifests are
    /// skipped and recorded in [`Registry::warnings`].
    pub fn load_from_dirs(dirs: &[PathBuf]) -> Registry {
        let mut reg = Registry::default();
        for dir in dirs {
            reg.scan_dir(dir);
        }
        reg
    }

    fn scan_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            // A missing dir isn't an error — it just contributes nothing.
            Err(_) => return,
        };
        // Sort for deterministic load order.
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
        let mut manifest: SidecarManifest = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => return self.warn(path, format!("invalid JSON/shape: {e}")),
        };
        if let Err(reason) = manifest.validate() {
            return self.warn(path, reason);
        }
        manifest.source_dir = path.parent().unwrap_or(path).to_path_buf();
        // Later dir wins by id (user overrides bundled).
        self.by_id.insert(manifest.id.clone(), manifest);
    }

    fn warn(&mut self, path: &Path, reason: String) {
        self.warnings.push(LoadWarning {
            path: path.to_path_buf(),
            reason,
        });
    }

    pub fn get(&self, id: &str) -> Option<&SidecarManifest> {
        self.by_id.get(id)
    }

    /// All loaded manifests, in id order.
    pub fn all(&self) -> impl Iterator<Item = &SidecarManifest> {
        self.by_id.values()
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Manifests skipped during loading, with reasons.
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

    fn valid_manifest(id: &str, version: &str) -> String {
        format!(
            r#"{{
              "schema_version": 1,
              "id": "{id}",
              "name": "Test Sidecar",
              "version": "{version}",
              "contract_version": {{ "major": 1, "minor": 0 }},
              "entry": {{
                "windows": {{ "command": "test.exe", "args": ["--serve"] }},
                "macos":   {{ "command": "test" }},
                "linux":   {{ "command": "test" }}
              }},
              "capabilities": ["context", "secrets"]
            }}"#
        )
    }

    #[test]
    fn loads_a_valid_manifest_and_resolves_capabilities() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "sidecar.json", &valid_manifest("ai-console", "0.1.0"));
        let reg = Registry::load_from_dirs(&[d.path().to_path_buf()]);
        assert_eq!(reg.len(), 1);
        let m = reg.get("ai-console").unwrap();
        assert_eq!(m.capabilities, vec![Capability::Context, Capability::Secrets]);
        assert!(m.entry_for_current_os().is_some());
        assert!(reg.warnings().is_empty());
    }

    #[test]
    fn skips_malformed_without_failing_the_scan() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "good.json", &valid_manifest("good", "1.0.0"));
        write(d.path(), "bad.json", "{ not valid json ");
        let reg = Registry::load_from_dirs(&[d.path().to_path_buf()]);
        assert_eq!(reg.len(), 1, "the good one still loads");
        assert!(reg.get("good").is_some());
        assert_eq!(reg.warnings().len(), 1);
        assert!(reg.warnings()[0].reason.contains("invalid JSON"));
    }

    #[test]
    fn skips_incompatible_contract_major() {
        let d = tempfile::tempdir().unwrap();
        let m = r#"{
          "schema_version": 1, "id": "future", "name": "F", "version": "9.0.0",
          "contract_version": { "major": 99, "minor": 0 },
          "entry": { "linux": { "command": "x" }, "windows": { "command": "x" }, "macos": { "command": "x" } }
        }"#;
        write(d.path(), "future.json", m);
        let reg = Registry::load_from_dirs(&[d.path().to_path_buf()]);
        assert!(reg.is_empty());
        assert_eq!(reg.warnings().len(), 1);
        assert!(reg.warnings()[0].reason.contains("contract major"));
    }

    #[test]
    fn skips_unknown_future_schema_version() {
        let d = tempfile::tempdir().unwrap();
        let m = r#"{
          "schema_version": 999, "id": "newer", "name": "N", "version": "1.0.0",
          "contract_version": { "major": 1, "minor": 0 },
          "entry": { "linux": { "command": "x" }, "windows": { "command": "x" }, "macos": { "command": "x" } }
        }"#;
        write(d.path(), "newer.json", m);
        let reg = Registry::load_from_dirs(&[d.path().to_path_buf()]);
        assert!(reg.is_empty());
        assert!(reg.warnings()[0].reason.contains("schema_version"));
    }

    #[test]
    fn user_dir_overrides_bundled_by_id() {
        let bundled = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        write(bundled.path(), "s.json", &valid_manifest("shared", "1.0.0"));
        write(user.path(), "s.json", &valid_manifest("shared", "2.0.0"));
        // User dir scanned last → wins.
        let reg = Registry::load_from_dirs(&[
            bundled.path().to_path_buf(),
            user.path().to_path_buf(),
        ]);
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get("shared").unwrap().version, "2.0.0");
    }

    #[test]
    fn missing_directory_contributes_nothing() {
        let reg = Registry::load_from_dirs(&[PathBuf::from("/no/such/dir/really")]);
        assert!(reg.is_empty());
        assert!(reg.warnings().is_empty());
    }
}
