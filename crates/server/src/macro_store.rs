//! Action-macro persistence store (CPE-1033, epic CPE-739): a `macros.json` catalog — name →
//! [`ActionMacro`] — in the app config dir reached through a [`ServerCtx`], tested via
//! [`crate::ctx::HeadlessCtx`]. Mirrors the `templates.json` store pattern in
//! [`crate::folder_template`] exactly, so the eventual Tauri commands are one-line dispatchers.
//!
//! This is distinct from [`crate::macro_library`]: that module is a pure, order-preserving,
//! case-insensitive-dedupe in-memory model with no disk I/O of its own. This module is the actual
//! on-disk store — a simple name-keyed [`Catalog`], matching how `folder_template` persists
//! `Template`s.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::action_macro::ActionMacro;
use crate::ctx::ServerCtx;

/// The stored macro catalog: name → [`ActionMacro`]. `BTreeMap` for a stable, diff-friendly
/// on-disk order.
pub type Catalog = BTreeMap<String, ActionMacro>;

/// A light summary of one stored macro: its name and how many steps it has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MacroSummary {
    pub name: String,
    pub steps: usize,
}

fn macros_path(dir: &Path) -> PathBuf {
    dir.join("macros.json")
}

/// Read the catalog from `macros.json` in `dir`; an absent or corrupt file yields an empty
/// catalog.
pub fn read_catalog_from(dir: &Path) -> Catalog {
    fs::read_to_string(macros_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the catalog to `macros.json` in `dir`, creating `dir` if needed.
pub fn write_catalog_to(dir: &Path, catalog: &Catalog) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(catalog).map_err(|e| e.to_string())?;
    fs::write(macros_path(dir), json.as_bytes()).map_err(|e| e.to_string())
}

/// Save (insert or replace by `name`) a macro and persist. Returns the updated catalog.
pub fn save(ctx: &dyn ServerCtx, macro_: ActionMacro) -> Result<Catalog, String> {
    let dir = ctx.app_config_dir()?;
    let mut catalog = read_catalog_from(&dir);
    catalog.insert(macro_.name.clone(), macro_);
    write_catalog_to(&dir, &catalog)?;
    Ok(catalog)
}

/// Every stored macro's name + step count, name-sorted (for a gallery/list UI).
pub fn list(ctx: &dyn ServerCtx) -> Result<Vec<MacroSummary>, String> {
    let catalog = read_catalog_from(&ctx.app_config_dir()?);
    Ok(catalog
        .values()
        .map(|m| MacroSummary {
            name: m.name.clone(),
            steps: m.steps.len(),
        })
        .collect())
}

/// One macro by name (`None` if absent).
pub fn load(ctx: &dyn ServerCtx, name: &str) -> Result<Option<ActionMacro>, String> {
    Ok(read_catalog_from(&ctx.app_config_dir()?).get(name).cloned())
}

/// Remove a macro by name and persist. Returns the updated catalog.
pub fn delete(ctx: &dyn ServerCtx, name: &str) -> Result<Catalog, String> {
    let dir = ctx.app_config_dir()?;
    let mut catalog = read_catalog_from(&dir);
    catalog.remove(name);
    write_catalog_to(&dir, &catalog)?;
    Ok(catalog)
}

/// A single macro's JSON, for sharing/export. `import` accepts exactly this.
pub fn export(macro_: &ActionMacro) -> Result<String, String> {
    serde_json::to_string_pretty(macro_).map_err(|e| e.to_string())
}

/// Import a macro — either a single [`ActionMacro`] JSON or a whole [`Catalog`] JSON — merged
/// into the store **by name** (a same-named macro is replaced). Returns the updated catalog.
pub fn import(ctx: &dyn ServerCtx, json: &str) -> Result<Catalog, String> {
    let dir = ctx.app_config_dir()?;
    let mut catalog = read_catalog_from(&dir);
    // Try a single macro first (a catalog fails this because its values aren't a bare macro).
    if let Ok(m) = serde_json::from_str::<ActionMacro>(json) {
        catalog.insert(m.name.clone(), m);
    } else if let Ok(incoming) = serde_json::from_str::<Catalog>(json) {
        catalog.extend(incoming);
    } else {
        return Err("invalid macro JSON".to_string());
    }
    write_catalog_to(&dir, &catalog)?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_macro::MacroStep;
    use crate::ctx::HeadlessCtx;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-macro-{tag}"))
    }

    fn sample(name: &str) -> ActionMacro {
        ActionMacro {
            name: name.into(),
            steps: vec![
                MacroStep::Tag {
                    label: "done".into(),
                },
                MacroStep::Move {
                    dest: "/archive".into(),
                },
            ],
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let base = scratch("roundtrip");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        let m = sample("tidy");
        save(&ctx, m.clone()).unwrap();
        assert_eq!(load(&ctx, "tidy").unwrap(), Some(m));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn save_replaces_same_named_macro_and_list_reflects_it() {
        let base = scratch("list");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        assert!(list(&ctx).unwrap().is_empty());

        save(&ctx, sample("tidy")).unwrap();
        let summaries = list(&ctx).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(
            summaries[0],
            MacroSummary {
                name: "tidy".into(),
                steps: 2
            }
        );

        // Save replaces a same-named macro rather than duplicating it.
        let mut replaced = sample("tidy");
        replaced.steps.truncate(1);
        save(&ctx, replaced).unwrap();
        let summaries = list(&ctx).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].steps, 1);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn delete_removes_a_macro() {
        let base = scratch("delete");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        save(&ctx, sample("a")).unwrap();
        save(&ctx, sample("b")).unwrap();

        delete(&ctx, "a").unwrap();
        assert!(load(&ctx, "a").unwrap().is_none());
        assert!(load(&ctx, "b").unwrap().is_some());

        let names: Vec<String> = list(&ctx).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["b".to_string()]);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn import_inserts_single_macro_and_whole_catalog_and_rejects_garbage() {
        let base = scratch("import");
        let ctx = HeadlessCtx::new(base.to_path_buf());

        let m = sample("solo");
        let json = export(&m).unwrap();
        import(&ctx, &json).unwrap();
        assert_eq!(load(&ctx, "solo").unwrap(), Some(m));

        let catalog: Catalog = [("a".to_string(), sample("a")), ("b".to_string(), sample("b"))]
            .into_iter()
            .collect();
        import(&ctx, &serde_json::to_string(&catalog).unwrap()).unwrap();
        let names: Vec<String> = list(&ctx).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["a".to_string(), "b".to_string(), "solo".to_string()]);

        assert!(import(&ctx, "not json").is_err());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn unknown_name_load_is_none() {
        let base = scratch("unknown");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        save(&ctx, sample("known")).unwrap();
        assert!(load(&ctx, "nope").unwrap().is_none());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_catalog_reads_as_empty() {
        let ctx_base = scratch("empty");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
        assert!(list(&ctx).unwrap().is_empty());
        assert!(load(&ctx, "nope").unwrap().is_none());
    }

    #[test]
    fn corrupt_catalog_file_is_tolerated_as_empty() {
        let base = scratch("corrupt");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        let dir = ctx.app_config_dir().unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(macros_path(&dir), b"{ not valid json ]").unwrap();

        // A corrupt file never panics or errors the read — it's treated as an empty catalog.
        assert!(list(&ctx).unwrap().is_empty());
        assert!(load(&ctx, "anything").unwrap().is_none());

        // And a subsequent save overwrites the corruption cleanly.
        save(&ctx, sample("fresh")).unwrap();
        assert_eq!(load(&ctx, "fresh").unwrap().unwrap().name, "fresh");
        let _ = fs::remove_dir_all(&base);
    }
}
