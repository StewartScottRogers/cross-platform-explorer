//! Select-by-pattern engine (CPE-1025, epic CPE-711): given a directory listing and a query, decide
//! which entries match — a shell-style glob, an extension list, "all files"/"all folders" by kind, or
//! the complement of another query (`Invert`). Pure: no filesystem access, so it's unit-testable without
//! touching disk and reusable by both a `#[tauri::command]` dispatcher and any future batch-select UI.
//!
//! Glob reuse note: the raw `*`/`?` matcher lives in [`crate::glob::glob_is_match`] (CPE-1302), shared
//! with `name_search.rs` (which used to carry a byte-identical private copy). It is **anchored** to the
//! whole name — deliberately unlike `name_search`'s public `name_matches`, which folds in a *substring*
//! fallback for patterns without wildcard characters (a literal `SelQuery::Glob("readme.txt")` must match
//! only the exact name, not any name merely *containing* "readme.txt"). This module keeps its own
//! case-insensitive `glob_is_match` wrapper that lowercases both sides before delegating to the shared
//! matcher, preserving its long-standing case-insensitive-anchored semantics.

/// One listing entry as far as selection cares: its name and whether it's a folder.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SelEntry {
    pub name: String,
    pub is_dir: bool,
}

/// A select-by-pattern query.
///
/// Externally-tagged (serde default) — **not** internally-tagged like `shell_menu::AppliesTo`: this
/// enum has newtype variants wrapping non-map values (`Glob(String)`, `Extension(Vec<String>)`) and a
/// recursive variant (`Invert(Box<SelQuery>)`), which internal tagging (`tag = "..."`) can neither
/// serialize nor even compile a `Serialize` impl for. External tagging handles all of them, so this type
/// is IPC-ready for a future `#[tauri::command]`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum SelQuery {
    /// Shell-style `*`/`?` glob over the whole entry name, case-insensitive, anchored to the full name.
    Glob(String),
    /// Names whose extension (lower-cased, no dot) is in this list. Folders never match.
    Extension(Vec<String>),
    /// Every file (folders excluded).
    AllFiles,
    /// Every folder (files excluded).
    AllFolders,
    /// The complement of the inner query over the same listing (order preserved).
    Invert(Box<SelQuery>),
}

/// Anchored wildcard match: `*` matches any run of characters (including none), `?` exactly one
/// character. Case-insensitive — lowercases both sides, then delegates to the shared, case-sensitive
/// [`crate::glob::glob_is_match`] (CPE-1302). Semantics unchanged from the former private copy.
fn glob_is_match(name: &str, pattern: &str) -> bool {
    crate::glob::glob_is_match(&name.to_lowercase(), &pattern.to_lowercase())
}

/// Lower-cased extension of `name` with no leading dot, or `None` for an extensionless name (or a
/// leading-dot dotfile, where the leading `.` isn't treated as an extension separator).
fn ext_of(name: &str) -> Option<String> {
    name.rfind('.').filter(|&d| d > 0).map(|d| name[d + 1..].to_ascii_lowercase())
}

fn entry_matches(entry: &SelEntry, query: &SelQuery) -> bool {
    match query {
        SelQuery::Glob(pattern) => glob_is_match(&entry.name, pattern),
        SelQuery::Extension(exts) => {
            if entry.is_dir {
                return false;
            }
            let want: Vec<String> = exts.iter().map(|e| e.trim_start_matches('.').to_ascii_lowercase()).collect();
            ext_of(&entry.name).map(|e| want.contains(&e)).unwrap_or(false)
        }
        SelQuery::AllFiles => !entry.is_dir,
        SelQuery::AllFolders => entry.is_dir,
        SelQuery::Invert(inner) => !entry_matches(entry, inner),
    }
}

/// Select which `entries` match `query`, returning their indices in listing order.
pub fn select(entries: &[SelEntry], query: &SelQuery) -> Vec<usize> {
    entries.iter().enumerate().filter(|(_, e)| entry_matches(e, query)).map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> SelEntry {
        SelEntry { name: name.into(), is_dir: false }
    }
    fn dir(name: &str) -> SelEntry {
        SelEntry { name: name.into(), is_dir: true }
    }
    fn names<'a>(entries: &'a [SelEntry], idxs: &[usize]) -> Vec<&'a str> {
        idxs.iter().map(|&i| entries[i].name.as_str()).collect()
    }

    #[test]
    fn glob_matches_case_insensitively_and_is_anchored_to_the_whole_name() {
        let entries = [file("Report.RS"), file("main.rs"), file("main.rs.bak"), dir("rs")];
        let got = select(&entries, &SelQuery::Glob("*.rs".into()));
        // Anchored: "main.rs.bak" doesn't end the pattern; case-insensitive: "Report.RS" still matches.
        assert_eq!(names(&entries, &got), vec!["Report.RS", "main.rs"]);
    }

    #[test]
    fn glob_question_mark_matches_exactly_one_character() {
        let entries = [file("a1b.txt"), file("a12b.txt"), file("ab.txt")];
        let got = select(&entries, &SelQuery::Glob("a?b.txt".into()));
        assert_eq!(names(&entries, &got), vec!["a1b.txt"]);
    }

    #[test]
    fn glob_literal_pattern_matches_only_the_exact_name() {
        let entries = [file("readme.txt"), file("my-readme.txt-notes"), file("README.TXT")];
        let got = select(&entries, &SelQuery::Glob("readme.txt".into()));
        // No wildcards ⇒ exact (case-insensitive) match, not a substring match.
        assert_eq!(names(&entries, &got), vec!["readme.txt", "README.TXT"]);
    }

    #[test]
    fn extension_matches_case_insensitively_and_folders_never_match() {
        let entries = [file("a.PNG"), file("b.jpg"), file("c.gif"), dir("d.png")];
        let got = select(&entries, &SelQuery::Extension(vec!["png".into(), ".JPG".into()]));
        assert_eq!(names(&entries, &got), vec!["a.PNG", "b.jpg"]);
    }

    #[test]
    fn all_files_and_all_folders_split_a_mixed_listing_by_kind() {
        let entries = [file("a.txt"), dir("sub"), file("b.txt"), dir("sub2")];
        assert_eq!(names(&entries, &select(&entries, &SelQuery::AllFiles)), vec!["a.txt", "b.txt"]);
        assert_eq!(names(&entries, &select(&entries, &SelQuery::AllFolders)), vec!["sub", "sub2"]);
    }

    #[test]
    fn invert_returns_the_exact_complement_preserving_listing_order() {
        let entries = [file("a.rs"), dir("sub"), file("b.txt"), file("c.rs")];
        let glob = SelQuery::Glob("*.rs".into());
        let direct = select(&entries, &glob);
        let inverted = select(&entries, &SelQuery::Invert(Box::new(glob)));
        // Every index appears in exactly one of the two sets, and both stay in listing order.
        assert_eq!(direct, vec![0, 3]);
        assert_eq!(inverted, vec![1, 2]);
        let mut union: Vec<usize> = direct.iter().chain(inverted.iter()).copied().collect();
        union.sort_unstable();
        assert_eq!(union, (0..entries.len()).collect::<Vec<_>>());
    }

    #[test]
    fn invert_of_all_files_selects_the_folders_and_vice_versa() {
        let entries = [dir("d1"), file("f1.txt"), dir("d2")];
        let got = select(&entries, &SelQuery::Invert(Box::new(SelQuery::AllFiles)));
        assert_eq!(names(&entries, &got), vec!["d1", "d2"]);
    }

    #[test]
    fn every_query_variant_round_trips_through_serde_json() {
        // The type is IPC-ready: externally-tagged serde serializes and deserializes every variant,
        // including the newtype (String/Vec) and recursive (Box<SelQuery>) ones, back to an equal value.
        let queries = [
            SelQuery::Glob("*.rs".into()),
            SelQuery::Extension(vec!["png".into(), "jpg".into()]),
            SelQuery::AllFiles,
            SelQuery::AllFolders,
            SelQuery::Invert(Box::new(SelQuery::Glob("a?b*".into()))),
        ];
        for q in queries {
            let json = serde_json::to_string(&q).expect("serialize");
            let back: SelQuery = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, q, "round-trip mismatch via {json}");
        }
    }

    #[test]
    fn empty_listing_and_no_match_yield_an_empty_selection() {
        let entries: [SelEntry; 0] = [];
        assert!(select(&entries, &SelQuery::AllFiles).is_empty());
        let entries = [file("a.txt")];
        assert!(select(&entries, &SelQuery::Glob("*.rs".into())).is_empty());
    }
}
