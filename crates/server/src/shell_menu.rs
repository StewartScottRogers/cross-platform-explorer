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
}
