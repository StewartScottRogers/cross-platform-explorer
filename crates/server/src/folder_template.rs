//! Folder templates & scaffolding core (CPE-835, epic CPE-740): capture a folder's structure as a
//! reusable [`Template`] and [`stamp`] it back out with `{token}` substitution.
//!
//! Kills the repetitive "create the same six subfolders again" chore. The model is a serde-JSON tree
//! ([`Template`] of [`Node`]s), so import/export is just that JSON. Pure + Tauri-free — the persistence
//! and Tauri commands (CPE-836) and the gallery/"New from template…" UI (CPE-837) build on this.
//!
//! Substitution is driven by a caller-supplied variable map applied to folder names, file names, and
//! file contents, so the core stays pure/deterministic; the command layer fills the vocabulary
//! (`{date}`, `{name}`, `{counter}`, …). Stamping is path-safe (a token value can never escape the
//! destination) and never clobbers an existing file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ctx::ServerCtx;

/// Placeholder files up to this size have their contents captured (as boilerplate); larger or non-UTF-8
/// files become empty placeholders, keeping a template small and text-only.
const MAX_CAPTURED_FILE: u64 = 64 * 1024;

/// A node in a template tree: a directory (with children) or a file (with placeholder contents).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    Dir {
        name: String,
        #[serde(default)]
        children: Vec<Node>,
    },
    File {
        name: String,
        #[serde(default)]
        contents: String,
    },
}

/// A captured folder structure that can be stamped out on demand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<Node>,
}

/// Capture `root`'s structure into a [`Template`]. Small UTF-8 files keep their contents; larger or
/// non-text files become empty placeholders; unreadable entries are skipped (like `list_dir`). A
/// non-folder `root` is an error.
pub fn capture(root: &Path, name: impl Into<String>) -> Result<Template, String> {
    let meta = fs::metadata(root).map_err(|e| format!("{}: {e}", root.display()))?;
    if !meta.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    Ok(Template {
        name: name.into(),
        nodes: capture_children(root),
    })
}

fn capture_children(dir: &Path) -> Vec<Node> {
    let mut entries: Vec<fs::DirEntry> = match fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return Vec::new(), // skip an unreadable directory rather than fail the capture
    };
    // Stable, name-sorted order so a captured template is deterministic.
    entries.sort_by_key(|e| e.file_name());

    let mut nodes = Vec::new();
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        match fs::metadata(&path) {
            Ok(m) if m.is_dir() => nodes.push(Node::Dir {
                name,
                children: capture_children(&path),
            }),
            Ok(m) => {
                let contents = if m.len() <= MAX_CAPTURED_FILE {
                    // Non-UTF-8 reads fall back to an empty placeholder.
                    fs::read_to_string(&path).unwrap_or_default()
                } else {
                    String::new()
                };
                nodes.push(Node::File { name, contents });
            }
            Err(_) => {} // skip an entry we can't stat
        }
    }
    nodes
}

/// Replace `{key}` tokens in `s` from `vars`. An unknown `{token}` is left verbatim (a template author may
/// want literal braces for a downstream tool). Applies uniformly to names and contents.
pub fn substitute(s: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        match rest[open + 1..].find('}') {
            Some(close_rel) => {
                let key = &rest[open + 1..open + 1 + close_rel];
                match vars.get(key) {
                    Some(v) => out.push_str(v),
                    None => {
                        // Unknown token — emit it verbatim, braces included.
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &rest[open + 1 + close_rel + 1..];
            }
            None => {
                // No closing brace: the remainder is literal.
                out.push_str(&rest[open..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Keep a stamped name a single path component: separators become `_` and a bare `..` is neutralised, so
/// a token value can never make the stamp escape the destination folder.
fn sanitize_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    if cleaned == ".." {
        "__".to_string()
    } else {
        cleaned
    }
}

/// Stamp `template` into `dest`, substituting `vars` in every folder name, file name, and file body.
/// Creates `dest` if needed. Path-safe (names are sanitized to single components) and non-destructive
/// (refuses to overwrite an existing file). Returns the created paths in creation order.
pub fn stamp(
    template: &Template,
    dest: &Path,
    vars: &BTreeMap<String, String>,
) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut created = Vec::new();
    stamp_nodes(&template.nodes, dest, vars, &mut created)?;
    Ok(created)
}

fn stamp_nodes(
    nodes: &[Node],
    dir: &Path,
    vars: &BTreeMap<String, String>,
    created: &mut Vec<PathBuf>,
) -> Result<(), String> {
    for node in nodes {
        match node {
            Node::Dir { name, children } => {
                let sub = dir.join(sanitize_component(&substitute(name, vars)));
                fs::create_dir_all(&sub).map_err(|e| format!("{}: {e}", sub.display()))?;
                created.push(sub.clone());
                stamp_nodes(children, &sub, vars, created)?;
            }
            Node::File { name, contents } => {
                let file = dir.join(sanitize_component(&substitute(name, vars)));
                if file.exists() {
                    return Err(format!(
                        "refusing to overwrite existing file: {}",
                        file.display()
                    ));
                }
                fs::write(&file, substitute(contents, vars))
                    .map_err(|e| format!("{}: {e}", file.display()))?;
                created.push(file);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The template store (CPE-836) — a `templates.json` catalog in the config dir, reached through a
// `ServerCtx`, following the `tags`/`settings` store pattern so the Tauri commands (CPE-837) are
// one-line dispatchers.
// ---------------------------------------------------------------------------

/// The stored template catalog: name → [`Template`]. `BTreeMap` for a stable, diff-friendly on-disk
/// order.
pub type Catalog = BTreeMap<String, Template>;

/// A gallery summary of one stored template: its name and how many dirs/files it stamps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateSummary {
    pub name: String,
    pub dirs: usize,
    pub files: usize,
}

fn templates_path(dir: &Path) -> PathBuf {
    dir.join("templates.json")
}

/// Read the catalog from `templates.json` in `dir`; an absent or corrupt file yields an empty catalog.
pub fn read_catalog_from(dir: &Path) -> Catalog {
    fs::read_to_string(templates_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the catalog to `templates.json` in `dir`, creating `dir` if needed.
pub fn write_catalog_to(dir: &Path, catalog: &Catalog) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(catalog).map_err(|e| e.to_string())?;
    fs::write(templates_path(dir), json.as_bytes()).map_err(|e| e.to_string())
}

fn count_nodes(nodes: &[Node], dirs: &mut usize, files: &mut usize) {
    for n in nodes {
        match n {
            Node::Dir { children, .. } => {
                *dirs += 1;
                count_nodes(children, dirs, files);
            }
            Node::File { .. } => *files += 1,
        }
    }
}

/// Save (insert or replace by `name`) a template and persist. Returns the updated catalog.
pub fn save(ctx: &dyn ServerCtx, template: Template) -> Result<Catalog, String> {
    let dir = ctx.app_config_dir()?;
    let mut catalog = read_catalog_from(&dir);
    catalog.insert(template.name.clone(), template);
    write_catalog_to(&dir, &catalog)?;
    Ok(catalog)
}

/// Every stored template's name + node counts, name-sorted (for the gallery).
pub fn list(ctx: &dyn ServerCtx) -> Result<Vec<TemplateSummary>, String> {
    let catalog = read_catalog_from(&ctx.app_config_dir()?);
    Ok(catalog
        .values()
        .map(|t| {
            let (mut dirs, mut files) = (0, 0);
            count_nodes(&t.nodes, &mut dirs, &mut files);
            TemplateSummary {
                name: t.name.clone(),
                dirs,
                files,
            }
        })
        .collect())
}

/// One template by name (`None` if absent).
pub fn load(ctx: &dyn ServerCtx, name: &str) -> Result<Option<Template>, String> {
    Ok(read_catalog_from(&ctx.app_config_dir()?).get(name).cloned())
}

/// Remove a template by name and persist. Returns the updated catalog.
pub fn delete(ctx: &dyn ServerCtx, name: &str) -> Result<Catalog, String> {
    let dir = ctx.app_config_dir()?;
    let mut catalog = read_catalog_from(&dir);
    catalog.remove(name);
    write_catalog_to(&dir, &catalog)?;
    Ok(catalog)
}

/// A single template's JSON, for sharing/export. `import` accepts exactly this.
pub fn export(template: &Template) -> Result<String, String> {
    serde_json::to_string_pretty(template).map_err(|e| e.to_string())
}

/// Import a template — either a single [`Template`] JSON or a whole [`Catalog`] JSON — merged into the
/// store **by name** (a same-named template is replaced). Returns the updated catalog.
pub fn import(ctx: &dyn ServerCtx, json: &str) -> Result<Catalog, String> {
    let dir = ctx.app_config_dir()?;
    let mut catalog = read_catalog_from(&dir);
    // Try a single template first (a catalog fails this because its values aren't a bare template).
    if let Ok(t) = serde_json::from_str::<Template>(json) {
        catalog.insert(t.name.clone(), t);
    } else if let Ok(incoming) = serde_json::from_str::<Catalog>(json) {
        catalog.extend(incoming);
    } else {
        return Err("invalid template JSON".to_string());
    }
    write_catalog_to(&dir, &catalog)?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::HeadlessCtx;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-tmpl-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn substitute_replaces_known_and_keeps_unknown() {
        let v = vars(&[("name", "Acme"), ("date", "2026-07-21")]);
        assert_eq!(substitute("{name}_{date}", &v), "Acme_2026-07-21");
        assert_eq!(substitute("{name}/{unknown}", &v), "Acme/{unknown}");
        assert_eq!(substitute("no tokens here", &v), "no tokens here");
        assert_eq!(substitute("dangling {brace", &v), "dangling {brace");
    }

    #[test]
    fn template_json_round_trips() {
        let t = Template {
            name: "proj".into(),
            nodes: vec![Node::Dir {
                name: "src".into(),
                children: vec![Node::File {
                    name: "main.rs".into(),
                    contents: "// {name}".into(),
                }],
            }],
        };
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Template>(&json).unwrap(), t);
    }

    #[test]
    fn capture_then_stamp_round_trips_structure_and_substitutes() {
        // Build a source folder: src/main.rs (with a token) + README + docs/ (empty).
        let src = scratch("src");
        fs::create_dir_all(src.join("src")).unwrap();
        fs::create_dir_all(src.join("docs")).unwrap();
        fs::write(src.join("src/main.rs"), b"// project {name}\n").unwrap();
        fs::write(src.join("README.md"), b"# {name}\n").unwrap();

        let tmpl = capture(&src, "starter").unwrap();
        assert_eq!(tmpl.name, "starter");

        // Stamp into a fresh dest with a project name, using a token in a folder name too.
        let mut t2 = tmpl.clone();
        t2.nodes.push(Node::Dir { name: "{name}-notes".into(), children: vec![] });

        let dest = scratch("dest");
        let created = stamp(&t2, &dest, &vars(&[("name", "Acme")])).unwrap();
        assert!(!created.is_empty());

        // Structure + content came through, with substitution applied to names and file bodies.
        assert!(dest.join("src/main.rs").is_file());
        assert!(dest.join("docs").is_dir());
        assert!(dest.join("Acme-notes").is_dir());
        assert_eq!(fs::read_to_string(dest.join("src/main.rs")).unwrap(), "// project Acme\n");
        assert_eq!(fs::read_to_string(dest.join("README.md")).unwrap(), "# Acme\n");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn stamp_is_path_safe_against_traversal() {
        // A malicious token value with separators / .. must stay a single component under dest.
        let tmpl = Template {
            name: "t".into(),
            nodes: vec![Node::Dir { name: "{evil}".into(), children: vec![] }],
        };
        let dest = scratch("safe");
        stamp(&tmpl, &dest, &vars(&[("evil", "../escaped")])).unwrap();
        // Nothing was created outside dest; the separator was neutralised into one component.
        assert!(!dest.parent().unwrap().join("escaped").exists());
        assert!(dest.join(".._escaped").is_dir());
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn stamp_refuses_to_overwrite_an_existing_file() {
        let dest = scratch("noclobber");
        fs::write(dest.join("keep.txt"), b"original").unwrap();
        let tmpl = Template {
            name: "t".into(),
            nodes: vec![Node::File { name: "keep.txt".into(), contents: "new".into() }],
        };
        assert!(stamp(&tmpl, &dest, &BTreeMap::new()).is_err());
        assert_eq!(fs::read_to_string(dest.join("keep.txt")).unwrap(), "original");
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn capture_rejects_a_non_folder() {
        let dir = scratch("nonfolder");
        let f = dir.join("file.txt");
        fs::write(&f, b"x").unwrap();
        assert!(capture(&f, "x").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    fn sample(name: &str) -> Template {
        Template {
            name: name.into(),
            nodes: vec![Node::Dir {
                name: "src".into(),
                children: vec![Node::File {
                    name: "main.rs".into(),
                    contents: String::new(),
                }],
            }],
        }
    }

    #[test]
    fn store_save_list_load_delete_round_trip() {
        let base = scratch("store");
        let ctx = HeadlessCtx::new(&base);
        assert!(list(&ctx).unwrap().is_empty());

        let t = sample("proj");
        save(&ctx, t.clone()).unwrap();

        let summaries = list(&ctx).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0], TemplateSummary { name: "proj".into(), dirs: 1, files: 1 });
        assert_eq!(load(&ctx, "proj").unwrap(), Some(t.clone()));

        // Save replaces a same-named template.
        let mut t2 = t.clone();
        t2.nodes.clear();
        save(&ctx, t2).unwrap();
        assert_eq!(load(&ctx, "proj").unwrap().unwrap().nodes.len(), 0);

        delete(&ctx, "proj").unwrap();
        assert!(load(&ctx, "proj").unwrap().is_none());
        assert!(list(&ctx).unwrap().is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn export_import_round_trips_and_rejects_garbage() {
        let base = scratch("io");
        let ctx = HeadlessCtx::new(&base);
        let t = sample("x");
        let json = export(&t).unwrap();
        import(&ctx, &json).unwrap();
        assert_eq!(load(&ctx, "x").unwrap(), Some(t));

        // Import also accepts a whole catalog JSON.
        let catalog: Catalog = [("a".to_string(), sample("a")), ("b".to_string(), sample("b"))]
            .into_iter()
            .collect();
        import(&ctx, &serde_json::to_string(&catalog).unwrap()).unwrap();
        let names: Vec<String> = list(&ctx).unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["a".to_string(), "b".to_string(), "x".to_string()]);

        assert!(import(&ctx, "not json").is_err());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_catalog_reads_as_empty() {
        let ctx = HeadlessCtx::new(scratch("empty"));
        assert!(list(&ctx).unwrap().is_empty());
        assert!(load(&ctx, "nope").unwrap().is_none());
    }
}
