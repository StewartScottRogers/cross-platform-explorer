//! Shared anchored-wildcard (glob) matcher (CPE-1302). A single `*`/`?` matcher used across the crate
//! so the same shell-style glob semantics back both the select-by-pattern engine ([`crate::selection`])
//! and filename search ([`crate::name_search`]) — the two used to carry byte-identical private copies of
//! this function — plus the tree-scan exclude-glob support (CPE-1302).
//!
//! [`glob_is_match`] is **case-sensitive** and does no lowercasing of its own: `*` matches any run of
//! characters (including none), `?` exactly one, and the pattern is **anchored** to the whole name (a
//! trailing unmatched run fails). Callers that want case-insensitive matching lowercase both `name` and
//! `pattern` before calling — that's exactly the difference between the two former call sites, and it's
//! preserved by each caller doing (or not doing) its own lowercasing. No regex dependency: an iterative
//! two-pointer backtracking scan.

/// Anchored wildcard match: `*` matches any run of characters (including none), `?` exactly one
/// character. **Case-sensitive** — `name` and `pattern` are compared as-is, so a case-insensitive
/// caller must lowercase both first. Anchored to the whole name. Iterative two-pointer backtracking,
/// no regex dependency. Never panics.
pub fn glob_is_match(name: &str, pattern: &str) -> bool {
    let n: Vec<char> = name.chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while i < n.len() {
        if j < p.len() && (p[j] == '?' || p[j] == n[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == '*' {
            star = Some(j);
            mark = i;
            j += 1;
        } else if let Some(s) = star {
            j = s + 1;
            mark += 1;
            i = mark;
        } else {
            return false;
        }
    }
    while j < p.len() && p[j] == '*' {
        j += 1;
    }
    j == p.len()
}

/// True iff the directory/entry base `name` matches ANY of the `excludes` globs, matched
/// **case-insensitively** and anchored to the whole name (via [`glob_is_match`] on the lowercased
/// forms). An **empty** `excludes` slice never matches — so a tree-scan caller passing `&[]` behaves
/// exactly as it did before exclude support existed (CPE-1302). Never panics.
pub fn any_glob_matches(name: &str, excludes: &[String]) -> bool {
    if excludes.is_empty() {
        return false;
    }
    let name_lower = name.to_lowercase();
    excludes.iter().any(|pat| glob_is_match(&name_lower, &pat.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact former private matcher from `name_search.rs` (assumes inputs already lowercased).
    fn old_name_search_glob(name: &str, pattern: &str) -> bool {
        let n: Vec<char> = name.chars().collect();
        let p: Vec<char> = pattern.chars().collect();
        let (mut i, mut j) = (0usize, 0usize);
        let (mut star, mut mark) = (None, 0usize);
        while i < n.len() {
            if j < p.len() && (p[j] == '?' || p[j] == n[i]) {
                i += 1;
                j += 1;
            } else if j < p.len() && p[j] == '*' {
                star = Some(j);
                mark = i;
                j += 1;
            } else if let Some(s) = star {
                j = s + 1;
                mark += 1;
                i = mark;
            } else {
                return false;
            }
        }
        while j < p.len() && p[j] == '*' {
            j += 1;
        }
        j == p.len()
    }

    /// The exact former private matcher from `selection.rs` (lowercases both inputs itself).
    fn old_selection_glob(name: &str, pattern: &str) -> bool {
        let n: Vec<char> = name.to_lowercase().chars().collect();
        let p: Vec<char> = pattern.to_lowercase().chars().collect();
        let (mut i, mut j) = (0usize, 0usize);
        let (mut star, mut mark) = (None, 0usize);
        while i < n.len() {
            if j < p.len() && (p[j] == '?' || p[j] == n[i]) {
                i += 1;
                j += 1;
            } else if j < p.len() && p[j] == '*' {
                star = Some(j);
                mark = i;
                j += 1;
            } else if let Some(s) = star {
                j = s + 1;
                mark += 1;
                i = mark;
            } else {
                return false;
            }
        }
        while j < p.len() && p[j] == '*' {
            j += 1;
        }
        j == p.len()
    }

    /// A representative spread of names × patterns exercising `*`, `?`, anchoring, empty strings,
    /// literal matches, and case — the shared matcher must agree with BOTH former private copies
    /// (name_search's pre-lowercased contract, and selection's lowercase-both contract).
    fn parity_cases() -> Vec<(&'static str, &'static str)> {
        let names = [
            "", "a", "ab", "abc", "photo.png", "photo.pngx", "a1b2.txt", "ab.txt", "anything",
            "IMG_2024.jpg", "Report.RS", "main.rs", "main.rs.bak", "readme.txt", "README.TXT",
            "node_modules", ".git", "target", "A?B",
        ];
        let patterns = [
            "", "*", "a", "a*", "*a", "*.png", "a?b?.txt", "img_*.jpg", "*.rs", "readme.txt",
            "node_modules", ".git", "target", "?", "a?b", "**", "*z*",
        ];
        let mut out = Vec::new();
        for n in names {
            for p in patterns {
                out.push((n, p));
            }
        }
        out
    }

    #[test]
    fn shared_matcher_matches_former_name_search_copy_on_prelowercased_inputs() {
        for (n, p) in parity_cases() {
            // name_search's contract: inputs are already lowercased, no lowercasing inside.
            let nl = n.to_lowercase();
            let pl = p.to_lowercase();
            assert_eq!(
                glob_is_match(&nl, &pl),
                old_name_search_glob(&nl, &pl),
                "name_search parity mismatch for name={nl:?} pattern={pl:?}"
            );
        }
    }

    #[test]
    fn shared_matcher_matches_former_selection_copy_when_caller_lowercases_both() {
        for (n, p) in parity_cases() {
            // selection's contract: it lowercases both itself, so the shared matcher fed the
            // lowercased forms must equal the old selection matcher fed the raw forms.
            assert_eq!(
                glob_is_match(&n.to_lowercase(), &p.to_lowercase()),
                old_selection_glob(n, p),
                "selection parity mismatch for name={n:?} pattern={p:?}"
            );
        }
    }

    #[test]
    fn any_glob_matches_empty_excludes_never_matches() {
        for (n, _) in parity_cases() {
            assert!(!any_glob_matches(n, &[]), "empty excludes must never match {n:?}");
        }
    }

    #[test]
    fn any_glob_matches_is_case_insensitive_and_anchored() {
        let ex = vec!["node_modules".to_string(), ".git".to_string(), "tar*".to_string()];
        assert!(any_glob_matches("node_modules", &ex));
        assert!(any_glob_matches("NODE_MODULES", &ex), "case-insensitive");
        assert!(any_glob_matches(".git", &ex));
        assert!(any_glob_matches("target", &ex), "tar* glob");
        assert!(any_glob_matches("TARGET", &ex));
        assert!(!any_glob_matches("node_modules_backup", &ex), "anchored: trailing chars fail literal");
        assert!(!any_glob_matches("src", &ex));
        assert!(!any_glob_matches("gitignore", &ex), "anchored: .git does not match gitignore");
    }
}
