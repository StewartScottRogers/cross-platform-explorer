//! Per-folder metadata-column config store (CPE-1032, epic CPE-707): remember which metadata
//! columns a user chose to show for a folder, and in what order.
//!
//! Mirrors the `folder_template.rs` store (CPE-836): a JSON catalog in the app config dir, reached
//! through [`ServerCtx`], with a tolerant read (an absent or corrupt file yields an empty catalog
//! rather than erroring). Kept decoupled from `column_extract`/`metadata_column` — this module only
//! persists string column-ids; the string↔`MetaColumn` mapping stays in the command/UI layer.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ctx::ServerCtx;

/// The ordered visible-column ids chosen for one folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ColumnConfig {
    pub columns: Vec<String>,
}

/// The stored config catalog: folder path → [`ColumnConfig`]. `BTreeMap` for a stable,
/// diff-friendly on-disk order.
pub type Catalog = BTreeMap<String, ColumnConfig>;

fn config_path(dir: &Path) -> PathBuf {
    dir.join("column_config.json")
}

/// Read the catalog from `column_config.json` in `dir`; an absent or corrupt file yields an empty
/// catalog.
fn read_catalog_from(dir: &Path) -> Catalog {
    fs::read_to_string(config_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the catalog to `column_config.json` in `dir`, creating `dir` if needed.
fn write_catalog_to(dir: &Path, catalog: &Catalog) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(catalog).map_err(|e| e.to_string())?;
    fs::write(config_path(dir), json.as_bytes()).map_err(|e| e.to_string())
}

/// The stored column config for `folder` (default empty if unset, or the catalog file is
/// absent/corrupt).
pub fn get(ctx: &dyn ServerCtx, folder: &str) -> ColumnConfig {
    let dir = match ctx.app_config_dir() {
        Ok(d) => d,
        Err(_) => return ColumnConfig::default(),
    };
    read_catalog_from(&dir).remove(folder).unwrap_or_default()
}

/// Save (insert or replace by folder path) a column config and persist. Preserves every other
/// folder's entry.
pub fn set(ctx: &dyn ServerCtx, folder: &str, config: ColumnConfig) -> Result<(), String> {
    let dir = ctx.app_config_dir()?;
    let mut catalog = read_catalog_from(&dir);
    catalog.insert(folder.to_string(), config);
    write_catalog_to(&dir, &catalog)
}

/// Remove `folder`'s stored column config and persist. Leaves every other folder's entry intact.
pub fn clear(ctx: &dyn ServerCtx, folder: &str) -> Result<(), String> {
    let dir = ctx.app_config_dir()?;
    let mut catalog = read_catalog_from(&dir);
    catalog.remove(folder);
    write_catalog_to(&dir, &catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::HeadlessCtx;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-colcfg-{tag}"))
    }

    #[test]
    fn set_then_get_round_trips() {
        let base = scratch("roundtrip");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        let cfg = ColumnConfig {
            columns: vec!["size".into(), "modified".into(), "kind".into()],
        };
        set(&ctx, "/photos", cfg.clone()).unwrap();
        assert_eq!(get(&ctx, "/photos"), cfg);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn unknown_folder_yields_default_empty() {
        let base = scratch("unknown");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        assert_eq!(get(&ctx, "/nope"), ColumnConfig::default());
        assert!(get(&ctx, "/nope").columns.is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn clear_removes_only_that_folder() {
        let base = scratch("clear");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        let a = ColumnConfig { columns: vec!["size".into()] };
        let b = ColumnConfig { columns: vec!["duration".into()] };
        set(&ctx, "/a", a.clone()).unwrap();
        set(&ctx, "/b", b.clone()).unwrap();

        clear(&ctx, "/a").unwrap();

        assert_eq!(get(&ctx, "/a"), ColumnConfig::default());
        assert_eq!(get(&ctx, "/b"), b);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn corrupt_catalog_file_is_tolerated() {
        let base = scratch("corrupt");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        let dir = ctx.app_config_dir().unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(config_path(&dir), b"{ this is not valid json").unwrap();

        assert_eq!(get(&ctx, "/anything"), ColumnConfig::default());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_catalog_file_is_tolerated() {
        let ctx_base = scratch("missing");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
        assert_eq!(get(&ctx, "/anything"), ColumnConfig::default());
    }

    #[test]
    fn set_overwrites_same_folder_entry() {
        let base = scratch("overwrite");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        set(&ctx, "/f", ColumnConfig { columns: vec!["size".into()] }).unwrap();
        set(&ctx, "/f", ColumnConfig { columns: vec!["kind".into(), "size".into()] }).unwrap();
        assert_eq!(
            get(&ctx, "/f"),
            ColumnConfig { columns: vec!["kind".into(), "size".into()] }
        );
        let _ = fs::remove_dir_all(&base);
    }
}
