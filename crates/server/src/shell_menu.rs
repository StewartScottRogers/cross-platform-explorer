//! Shell context-menu model (CPE-945, epic CPE-712): the pure applicability core for "CPE as a shell
//! citizen". A set of registered menu **verbs** (a label + a command template + which selections they
//! apply to) and a function that, given the current selection, returns the verbs to show. No OS shell
//! registration here (that's per-platform glue); this decides *what* to offer.

/// What kind of selection a verb applies to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "applies", rename_all = "snake_case")]
pub enum AppliesTo {
    /// Only when every selected item is a file.
    Files,
    /// Only when every selected item is a folder.
    Folders,
    /// Any selection (files, folders, or a mix).
    Any,
    /// Only when every selected item is a file with one of these (lower-cased, no-dot) extensions.
    Extensions(Vec<String>),
}

/// A registered context-menu verb.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MenuVerb {
    pub id: String,
    pub label: String,
    /// Command template run over the selection (e.g. `cpe open "{path}"`); expansion is the caller's job.
    pub command: String,
    pub applies: AppliesTo,
}

/// A selected item — just what applicability needs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SelItem {
    pub path: String,
    pub is_dir: bool,
}

impl SelItem {
    fn ext(&self) -> Option<String> {
        let name = self.path.rsplit(['/', '\\']).next().unwrap_or(&self.path);
        name.rfind('.').filter(|&d| d > 0).map(|d| name[d + 1..].to_ascii_lowercase())
    }
}

fn verb_applies(applies: &AppliesTo, sel: &[SelItem]) -> bool {
    if sel.is_empty() {
        return false; // no selection ⇒ no per-item verbs
    }
    match applies {
        AppliesTo::Any => true,
        AppliesTo::Files => sel.iter().all(|s| !s.is_dir),
        AppliesTo::Folders => sel.iter().all(|s| s.is_dir),
        AppliesTo::Extensions(exts) => {
            let want: Vec<String> = exts.iter().map(|e| e.trim_start_matches('.').to_ascii_lowercase()).collect();
            sel.iter().all(|s| !s.is_dir && s.ext().map(|e| want.contains(&e)).unwrap_or(false))
        }
    }
}

/// The verbs to show for `selection`, preserving the registration order. A verb shows only when it
/// applies to **every** selected item (so it's always meaningful for the whole selection).
pub fn verbs_for<'a>(verbs: &'a [MenuVerb], selection: &[SelItem]) -> Vec<&'a MenuVerb> {
    verbs.iter().filter(|v| verb_applies(&v.applies, selection)).collect()
}

// ── Windows shell registration plan (CPE-1019, epic CPE-712) ────────────────────────────────────────
// Turn the "Open in Cross-Platform Explorer" integration into an explicit list of registry operations, as
// *data*, so the apply glue (CPE-1020) stays trivial and the reversibility guarantee is unit-testable. No
// registry I/O here. Everything registers under HKCU\Software\Classes so no elevation is required.

/// One registry value to write when installing the Windows shell integration. `key` is the path **under
/// HKCU**; an empty `value_name` denotes the key's `(Default)` value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RegEntry {
    pub key: String,
    pub value_name: String,
    pub value: String,
}

/// The Windows shell-registration plan: values to write on install, and the full key paths to delete on
/// uninstall (deleting a `…\shell\CPE` root removes its `command` subkey with it).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct WinShellPlan {
    pub install: Vec<RegEntry>,
    pub remove: Vec<String>,
}

/// Build the HKCU registry plan for the "Open in CPE" context-menu integration.
///
/// Registers three file-explorer surfaces — on a folder, on the folder background (empty space in a
/// directory), and on a drive — each as `Software\Classes\<class>\shell\CPE` with a label + `Icon`, and a
/// `command` subkey invoking `exe_path` with the selected path. Folders/drives pass `"%1"`; the folder
/// background passes `"%V"` (the current directory). `exe_path` is quoted so spaces are safe.
pub fn windows_shell_plan(exe_path: &str, app_name: &str) -> WinShellPlan {
    // (registry class, path placeholder the shell substitutes).
    let surfaces = [("Directory", "%1"), (r"Directory\Background", "%V"), ("Drive", "%1")];
    let label = format!("Open in {app_name}");

    let mut install = Vec::new();
    let mut remove = Vec::new();
    for (class, placeholder) in surfaces {
        let root = format!(r"Software\Classes\{class}\shell\CPE");
        // Label (Default value of the verb key) + the icon shown beside it.
        install.push(RegEntry { key: root.clone(), value_name: String::new(), value: label.clone() });
        install.push(RegEntry { key: root.clone(), value_name: "Icon".into(), value: exe_path.to_string() });
        // The command run when the item is chosen.
        install.push(RegEntry {
            key: format!(r"{root}\command"),
            value_name: String::new(),
            value: format!(r#""{exe_path}" "{placeholder}""#),
        });
        remove.push(root);
    }
    WinShellPlan { install, remove }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verb(id: &str, applies: AppliesTo) -> MenuVerb {
        MenuVerb { id: id.into(), label: id.into(), command: format!("cpe {id} \"{{path}}\""), applies }
    }
    fn file(path: &str) -> SelItem {
        SelItem { path: path.into(), is_dir: false }
    }
    fn dir(path: &str) -> SelItem {
        SelItem { path: path.into(), is_dir: true }
    }

    #[test]
    fn any_always_applies_to_a_non_empty_selection() {
        let v = [verb("props", AppliesTo::Any)];
        assert_eq!(verbs_for(&v, &[file("a.txt"), dir("d")]).len(), 1);
        assert!(verbs_for(&v, &[]).is_empty()); // empty selection ⇒ nothing
    }

    #[test]
    fn files_and_folders_require_a_uniform_selection() {
        let vs = [verb("openfile", AppliesTo::Files), verb("openfolder", AppliesTo::Folders)];
        assert_eq!(verbs_for(&vs, &[file("a"), file("b")]).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), vec!["openfile"]);
        assert_eq!(verbs_for(&vs, &[dir("d1"), dir("d2")]).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), vec!["openfolder"]);
        assert!(verbs_for(&vs, &[file("a"), dir("d")]).is_empty()); // mixed ⇒ neither
    }

    #[test]
    fn extensions_match_every_selected_file_case_insensitively() {
        let v = [verb("edit-img", AppliesTo::Extensions(vec!["png".into(), ".JPG".into()]))];
        assert_eq!(verbs_for(&v, &[file("a.png"), file("b.JPG")]).len(), 1);
        assert!(verbs_for(&v, &[file("a.png"), file("c.gif")]).is_empty()); // one non-matching ⇒ hidden
        assert!(verbs_for(&v, &[dir("d.png")]).is_empty()); // a folder never matches an ext verb
    }

    #[test]
    fn registration_order_is_preserved() {
        let vs = [verb("z", AppliesTo::Any), verb("a", AppliesTo::Any)];
        assert_eq!(verbs_for(&vs, &[file("x")]).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), vec!["z", "a"]);
    }

    // ── Windows registration plan (CPE-1019) ──

    fn plan() -> WinShellPlan {
        windows_shell_plan(r"C:\Program Files\Cross-Platform Explorer\cpe.exe", "Cross-Platform Explorer")
    }

    fn cmd_for(p: &WinShellPlan, class: &str) -> String {
        let key = format!(r"Software\Classes\{class}\shell\CPE\command");
        p.install.iter().find(|e| e.key == key && e.value_name.is_empty()).map(|e| e.value.clone()).unwrap_or_default()
    }

    #[test]
    fn registers_the_three_explorer_surfaces_with_correct_command_placeholders() {
        let p = plan();
        // Folder and drive act on the clicked path (%1); background acts on the current dir (%V).
        assert_eq!(cmd_for(&p, "Directory"), r#""C:\Program Files\Cross-Platform Explorer\cpe.exe" "%1""#);
        assert_eq!(cmd_for(&p, "Drive"), r#""C:\Program Files\Cross-Platform Explorer\cpe.exe" "%1""#);
        assert_eq!(cmd_for(&p, r"Directory\Background"), r#""C:\Program Files\Cross-Platform Explorer\cpe.exe" "%V""#);
    }

    #[test]
    fn label_and_icon_come_from_args() {
        let p = plan();
        let root = r"Software\Classes\Directory\shell\CPE";
        let default = p.install.iter().find(|e| e.key == root && e.value_name.is_empty()).unwrap();
        assert_eq!(default.value, "Open in Cross-Platform Explorer");
        let icon = p.install.iter().find(|e| e.key == root && e.value_name == "Icon").unwrap();
        assert_eq!(icon.value, r"C:\Program Files\Cross-Platform Explorer\cpe.exe");
    }

    #[test]
    fn every_installed_root_key_is_removed_on_uninstall() {
        // Reversibility invariant: no residue. Every `…\shell\CPE` root an install entry touches must be in
        // the remove set (strip a trailing `\command` to get each entry's verb root).
        let p = plan();
        for e in &p.install {
            let root = e.key.strip_suffix(r"\command").unwrap_or(&e.key);
            assert!(p.remove.contains(&root.to_string()), "install key {} has no matching remove entry", e.key);
        }
        assert_eq!(p.remove.len(), 3); // exactly the three surface roots, nothing stray
    }

    #[test]
    fn exe_path_is_quoted_so_spaces_are_safe() {
        // The command string must keep the exe as a single quoted token even with spaces in the path.
        assert!(cmd_for(&plan(), "Directory").starts_with(r#""C:\Program Files\"#));
    }
}
