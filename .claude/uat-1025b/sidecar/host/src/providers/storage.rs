//! Storage namespace capability provider (CPE-269).
//!
//! Each sidecar gets a private, host-assigned directory for its own settings/state/
//! caches, isolated from the explorer's config and from every other sidecar. The
//! directory is derived from the requesting sidecar's id (which the broker supplies,
//! so a sidecar can only ever address its *own* namespace), created on first use, and
//! removable on uninstall.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sidecar_contract::{Capability, ContractError, ErrorCode, Request};

use crate::broker::CapabilityProvider;

/// Serves `Capability::Storage`, rooted at a host-owned base directory (e.g.
/// `<app-data>/sidecars`). Each sidecar's namespace is `base/<id>`.
pub struct StorageProvider {
    base: PathBuf,
}

impl StorageProvider {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    /// Resolve (without creating) the namespace dir for `sidecar_id`, rejecting any id
    /// that isn't a single safe path segment — so a crafted id can never escape `base`.
    pub fn namespace_path(&self, sidecar_id: &str) -> Result<PathBuf, String> {
        if !is_safe_segment(sidecar_id) {
            return Err(format!("unsafe sidecar id '{sidecar_id}'"));
        }
        Ok(self.base.join(sidecar_id))
    }

    /// Resolve and create the namespace dir.
    pub fn ensure_dir(&self, sidecar_id: &str) -> Result<PathBuf, String> {
        let dir = self.namespace_path(sidecar_id)?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        Ok(dir)
    }

    /// Remove a sidecar's namespace and everything in it (uninstall, CPE-276).
    pub fn clear(&self, sidecar_id: &str) -> Result<(), String> {
        let dir = self.namespace_path(sidecar_id)?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("remove {}: {e}", dir.display()))?;
        }
        Ok(())
    }
}

impl CapabilityProvider for StorageProvider {
    fn capability(&self) -> Capability {
        Capability::Storage
    }

    fn handle(&self, sidecar_id: &str, request: &Request) -> Result<Value, ContractError> {
        match request.method.as_str() {
            "storage.dir" => {
                let dir = self
                    .ensure_dir(sidecar_id)
                    .map_err(|e| ContractError::new(ErrorCode::Internal, e, false))?;
                Ok(json!({ "dir": dir.to_string_lossy() }))
            }
            other => Err(ContractError::new(
                ErrorCode::ToolFailure,
                format!("unknown storage method '{other}'"),
                false,
            )),
        }
    }
}

/// A valid single path segment: non-empty, no separators, not a dotted traversal.
/// Requires exactly one path component and that it be `Normal` — so `..`, `.`, `a/b`,
/// `a\b`, absolute paths, and Windows prefixes are all rejected.
fn is_safe_segment(s: &str) -> bool {
    if s.is_empty() || s.contains('/') || s.contains('\\') {
        return false;
    }
    let mut components = Path::new(s).components();
    matches!(components.next(), Some(std::path::Component::Normal(_))) && components.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_is_under_base_and_created() {
        let base = tempfile::tempdir().unwrap();
        let p = StorageProvider::new(base.path());
        let out = p
            .handle("ai-console", &Request { method: "storage.dir".into(), params: Value::Null })
            .unwrap();
        let dir = out["dir"].as_str().unwrap();
        assert!(Path::new(dir).is_dir());
        assert!(dir.contains("ai-console"));
        assert!(Path::new(dir).starts_with(base.path()));
    }

    #[test]
    fn different_sidecars_get_isolated_dirs() {
        let base = tempfile::tempdir().unwrap();
        let p = StorageProvider::new(base.path());
        let a = p.ensure_dir("alpha").unwrap();
        let b = p.ensure_dir("beta").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn a_traversal_id_is_rejected() {
        let base = tempfile::tempdir().unwrap();
        let p = StorageProvider::new(base.path());
        assert!(p.namespace_path("..").is_err());
        assert!(p.namespace_path("../../etc").is_err());
        assert!(p.namespace_path("a/b").is_err());
        assert!(p.namespace_path("a\\b").is_err());
        assert!(p.namespace_path("").is_err());
    }

    #[test]
    fn clear_removes_the_namespace() {
        let base = tempfile::tempdir().unwrap();
        let p = StorageProvider::new(base.path());
        let dir = p.ensure_dir("gone").unwrap();
        std::fs::write(dir.join("state.json"), b"{}").unwrap();
        assert!(dir.exists());
        p.clear("gone").unwrap();
        assert!(!dir.exists());
    }
}
