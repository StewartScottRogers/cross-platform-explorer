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
#[cfg_attr(feature = "specta", derive(specta::Type))]
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
#[cfg_attr(feature = "specta", derive(specta::Type))]
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

/// Replace `{key}` tokens in `s` from `vars`. A **token** is `{` + an identifier (ASCII letters, digits,
/// `_`) + `}`; an unknown token is left verbatim (a template author may want literal braces downstream).
/// Any other `{` — a code brace like `fn main() {`, a `{` followed by a space, an unmatched brace — is
/// literal, so tokens embedded inside braced content (e.g. `println!("{name}")`) are still substituted.
pub fn substitute(s: &str, vars: &BTreeMap<String, String>) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            // Read a run of identifier characters after the '{'.
            let mut j = i + 1;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            // A token needs a non-empty identifier immediately closed by '}'.
            if j > i + 1 && j < chars.len() && chars[j] == '}' {
                let key: String = chars[i + 1..j].iter().collect();
                match vars.get(&key) {
                    Some(v) => out.push_str(v),
                    None => out.extend(&chars[i..=j]), // unknown {token} verbatim
                }
                i = j + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
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
                // CPE-1705: was `if file.exists()`. The function's own contract two doc-comments up is
                // "non-destructive (refuses to overwrite an existing file)", and `Path::exists()` could
                // not deliver it — an unreadable slot answered `false` and `fs::write` truncated it.
                //
                // CPE-1770: `clobber_refusal` alone was still wrong here, for a DIFFERENT reason than a
                // rename-destructive site — this is a CREATE site (`fs::write` right below), not a clobber
                // site. `clobber_refusal` follows a symlink, so a **dangling** link at `file` reads as
                // free and `fs::write` then creates the link's target — the exact write-through
                // `create_slot_refusal` (CPE-1718) exists to close, checking the link question BEFORE
                // occupancy (the opposite order from a rename-destructive site) because at a create site a
                // **live** link is just as much a write-through hazard as a dangling one: `fs::write`
                // follows it and the caller believes it wrote `file`, when it actually wrote through to
                // wherever the link points.
                if let Some(e) = crate::fsutil::create_slot_refusal(
                    &file,
                    &format!("refusing to overwrite existing file: {}", file.display()),
                ) {
                    return Err(e);
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
#[cfg_attr(feature = "specta", derive(specta::Type))]
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

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-tmpl-{tag}"))
    }

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// CPE-1705 at `stamp`: the function's own contract is *"non-destructive (refuses to overwrite an
    /// existing file)"*, and `if file.exists()` could not deliver it — a slot whose stat failed answered
    /// `false`, the guard passed, and `fs::write` truncated whatever was really there.
    ///
    /// `write`-destructive, so — as at `join_files` — the ACL that hides the file also refuses the write,
    /// and a bare `expect_err` would pass against the unfixed code. Asserts on **which** error.
    ///
    /// **CPE-1770 changed which half answers this.** `stamp` now calls `create_slot_refusal`, whose link
    /// check runs FIRST (the opposite order from `clobber_refusal`/`rename_slot_refusal`), so a `symlink_metadata`
    /// failure on this ACL-denied slot is caught by `classify_create_slot`'s own `Unknown` arm before
    /// `clobber_refusal`'s ever runs — a different message than before ("could not check WHETHER … IS A
    /// LINK" vs. the old "could not check what is AT …"), same fail-closed verdict.
    #[test]
    fn cpe_1705_stamp_refuses_a_file_slot_it_cannot_stat() {
        use std::io::Write;
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the stamp unreadable-slot leg on this platform: the Unix deny \
                 mechanism chmods the PARENT, which refuses `create_dir_all`/`fs::write` for the whole \
                 template before the per-file guard is reached. NOTHING in this test covered that route \
                 on this run; `cpe_1705_stamp_still_refuses_a_readable_existing_file` carries the honest \
                 case on every OS."
            );
        }
        #[cfg(windows)]
        {
            let d = scratch("cpe1705-stamp-denied");
            let victim = d.join("README.md");
            fs::write(&victim, b"VICTIM ORIGINAL").unwrap();

            struct Restore<'a>(&'a Path, &'a Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    crate::fsutil::undo_deny_stat_of(self.0, self.1);
                    let _ = fs::remove_dir_all(self.1);
                }
            }
            let _r = Restore(&victim, &d);

            if !crate::fsutil::deny_stat_of(&victim) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1705] SKIPPED the stamp denied-slot leg: could not deny stat of {} on this \
                     machine. NOTHING in this test covered that route on this run.",
                    victim.display()
                );
                return;
            }

            let template = Template {
                name: "t".to_string(),
                nodes: vec![Node::File { name: "README.md".to_string(), contents: "NEW".to_string() }],
            };
            let err = stamp(&template, &d, &vars(&[]))
                .expect_err("a file slot we cannot stat must refuse, not be written over");
            assert!(
                err.contains("could not check whether") && err.contains("nothing was written"),
                "must be the guard's refusal, not an incidental write failure: {err}"
            );
        }
    }

    /// The ungated sibling, on every OS: the ordinary refusal keeps its wording, and a genuinely free
    /// slot is still stamped. A guard that refused everything would break every template.
    #[test]
    fn cpe_1705_stamp_still_refuses_a_readable_existing_file() {
        let d = scratch("cpe1705-stamp-ok");
        fs::write(d.join("README.md"), b"KEEP ME").unwrap();
        let template = Template {
            name: "t".to_string(),
            nodes: vec![Node::File { name: "README.md".to_string(), contents: "NEW".to_string() }],
        };
        let err = stamp(&template, &d, &vars(&[])).expect_err("an occupied slot must refuse");
        assert!(err.contains("refusing to overwrite existing file"), "{err}");
        assert_eq!(fs::read(d.join("README.md")).unwrap(), b"KEEP ME".to_vec());

        let fresh = scratch("cpe1705-stamp-fresh");
        let created = stamp(&template, &fresh, &vars(&[])).expect("a free slot must still be stamped");
        assert_eq!(created, vec![fresh.join("README.md")]);
        assert_eq!(fs::read(fresh.join("README.md")).unwrap(), b"NEW".to_vec());

        let _ = fs::remove_dir_all(&d);
        let _ = fs::remove_dir_all(&fresh);
    }

    /// **CPE-1770.** `stamp`'s own contract is "non-destructive"; `clobber_refusal` alone did not deliver
    /// that for a **dangling** link at the file slot: `Path::try_exists` follows the link, sees nothing at
    /// its (nonexistent) target, answers `Ok(false)`, and the pre-fix guard read that as free — so
    /// `fs::write` below wrote the template's boilerplate contents straight THROUGH the link, landing them
    /// wherever the link pointed (potentially outside the destination folder entirely) while `stamp`
    /// reported success. `create_slot_refusal` (CPE-1718) is the correct member of the guard family for a
    /// CREATE site — not `clobber_refusal` and not `clobber_refusal_link_aware` (that one is for a
    /// REFUSAL-shaped site that does not itself write; here the write is `fs::write` two lines below the
    /// guard, so the link question must be checked BEFORE occupancy, `create_slot_refusal`'s own order).
    ///
    /// Asserts the HARM — nothing landed at the link's phantom target, and the slot still holds the
    /// link, not real bytes — **before** trusting the `Result`, per this family's own failure mode: it
    /// fails by *succeeding*. Covers both the symlink and (unprivileged Windows) NTFS-junction leg via
    /// `make_dangling_link`'s own fallback. `d`'s `ScratchDir` drop guard is armed at construction, before
    /// any assertion runs.
    #[test]
    fn cpe_1770_stamp_refuses_a_dangling_link_at_the_file_slot_instead_of_writing_through_it() {
        use std::io::Write;
        let d = scratch("cpe1770-dangling-link");
        let file_slot = d.join("README.md");
        if !crate::fsutil::make_dangling_link(&file_slot) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1770] SKIPPED the stamp dangling-link leg: this machine could not stage a link or \
                 junction at {} at all. NOTHING in this test covered CPE-1770 on this run.",
                file_slot.display()
            );
            return;
        }
        let phantom_target = crate::fsutil::dangling_link_target(&file_slot);
        assert!(!phantom_target.exists(), "sanity: the link really is dangling before stamp runs");

        let template = Template {
            name: "t".to_string(),
            nodes: vec![Node::File { name: "README.md".to_string(), contents: "NEW".to_string() }],
        };
        let err = stamp(&template, &d, &vars(&[]))
            .expect_err("a dangling link at the file slot must be refused, not written through");

        // THE HARM, on the filesystem, before trusting the `Result`: nothing must have appeared at the
        // link's phantom target (a write following the link would land exactly there), and the slot must
        // still hold the untouched link, not the template's bytes written through it.
        assert!(
            !phantom_target.exists(),
            "the write must not have followed the dangling link to its (nonexistent) target"
        );
        assert!(
            fs::symlink_metadata(&file_slot).is_ok_and(|m| m.file_type().is_symlink()),
            "the file slot must still hold the link, not real content written through it"
        );
        // `create_slot_refusal` checks the LINK question first (the opposite order from a
        // rename-destructive site), so a dangling link gets its own honest wording — "writes THROUGH
        // it" — rather than the site's occupied-slot wording, which would falsely imply a real file sits
        // there.
        assert!(
            err.contains("is a link") && err.contains("writes THROUGH it"),
            "the refusal must say what is actually in the way and what would have happened: {err}"
        );

        let _ = fs::remove_dir_all(&d);
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
    fn substitute_handles_tokens_inside_code_braces() {
        // Regression (CPE-839): a token embedded in braced content — like source code — must still be
        // substituted; the surrounding `{`/`}` are literal, not a giant "unknown token" that swallows it.
        let v = vars(&[("name", "Acme")]);
        assert_eq!(
            substitute("fn main() { println!(\"{name}\"); }", &v),
            "fn main() { println!(\"Acme\"); }"
        );
        // A brace immediately followed by a space (or non-identifier) is literal.
        assert_eq!(substitute("if x { {name} } else {}", &v), "if x { Acme } else {}");
        // A `{...}` whose inside isn't a bare identifier is left untouched.
        assert_eq!(substitute("{a-b}", &v), "{a-b}");
        assert_eq!(substitute("{ name }", &v), "{ name }");
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
        let ctx = HeadlessCtx::new(base.to_path_buf());
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
        let ctx = HeadlessCtx::new(base.to_path_buf());
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
        let ctx_base = scratch("empty");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
        assert!(list(&ctx).unwrap().is_empty());
        assert!(load(&ctx, "nope").unwrap().is_none());
    }
}
