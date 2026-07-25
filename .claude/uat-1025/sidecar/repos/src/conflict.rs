//! Merge/rebase conflict-state parsing (CPE-496).
//!
//! When a two-way-mirror sync reconciles a divergence (merge or rebase) it can leave the working tree
//! with **unmerged** files. This module is the pure, testable core of the in-app conflict resolver: it
//! reads `git status --porcelain=v2` and reports each conflicted path with its **conflict kind** (both
//! modified, both added, deleted-by-us, …) from git's two-letter unmerged code. It performs no I/O and
//! decides nothing about *how* to resolve — the host reads the three versions and stages the result.

/// The kind of a single unmerged path, decoded from a porcelain-v2 `u <XY>` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// `UU` — both sides changed the file.
    BothModified,
    /// `AA` — both sides added the file (no common base).
    BothAdded,
    /// `DD` — both sides deleted it (rare; content already gone).
    BothDeleted,
    /// `AU` — we added, they didn't have it.
    AddedByUs,
    /// `UA` — they added, we didn't have it.
    AddedByThem,
    /// `DU` — we deleted, they modified.
    DeletedByUs,
    /// `UD` — they deleted, we modified.
    DeletedByThem,
    /// Any other/unknown unmerged code.
    Unknown,
}

impl ConflictKind {
    /// The git two-letter code this kind came from.
    pub fn from_code(code: &str) -> ConflictKind {
        match code {
            "UU" => ConflictKind::BothModified,
            "AA" => ConflictKind::BothAdded,
            "DD" => ConflictKind::BothDeleted,
            "AU" => ConflictKind::AddedByUs,
            "UA" => ConflictKind::AddedByThem,
            "DU" => ConflictKind::DeletedByUs,
            "UD" => ConflictKind::DeletedByThem,
            _ => ConflictKind::Unknown,
        }
    }

    /// A short human label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            ConflictKind::BothModified => "both modified",
            ConflictKind::BothAdded => "both added",
            ConflictKind::BothDeleted => "both deleted",
            ConflictKind::AddedByUs => "added by us",
            ConflictKind::AddedByThem => "added by them",
            ConflictKind::DeletedByUs => "deleted by us",
            ConflictKind::DeletedByThem => "deleted by them",
            ConflictKind::Unknown => "conflict",
        }
    }

    /// A stable snake_case code for the frontend.
    pub fn code(self) -> &'static str {
        match self {
            ConflictKind::BothModified => "both_modified",
            ConflictKind::BothAdded => "both_added",
            ConflictKind::BothDeleted => "both_deleted",
            ConflictKind::AddedByUs => "added_by_us",
            ConflictKind::AddedByThem => "added_by_them",
            ConflictKind::DeletedByUs => "deleted_by_us",
            ConflictKind::DeletedByThem => "deleted_by_them",
            ConflictKind::Unknown => "conflict",
        }
    }
}

/// One conflicted path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub path: String,
    pub kind: ConflictKind,
}

/// Parse the unmerged (`u`) records out of `git status --porcelain=v2 -z? no` output. Each unmerged
/// record is:
///   `u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>`
/// We need only the `XY` code and the path (which is the remainder of the line, so a path with spaces
/// is preserved). Non-`u` lines are ignored. Total + panic-free on any input.
pub fn parse_conflicts(porcelain_v2: &str) -> Vec<Conflict> {
    let mut out = Vec::new();
    for line in porcelain_v2.lines() {
        let Some(rest) = line.strip_prefix("u ") else { continue };
        // 10 space-separated fields: XY sub m1 m2 m3 mW h1 h2 h3 <path>. The path is the 10th chunk
        // (splitn keeps its internal spaces). Take the code, then skip the 8 middle fields to the path.
        let mut fields = rest.splitn(10, ' ');
        let code = fields.next().unwrap_or("");
        let path = fields.nth(8).unwrap_or("").to_string();
        if code.len() != 2 || path.is_empty() {
            continue;
        }
        out.push(Conflict { path, kind: ConflictKind::from_code(code) });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // A realistic porcelain-v2 unmerged record: u XY sub m1 m2 m3 mW h1 h2 h3 path
    fn u(code: &str, path: &str) -> String {
        format!("u {code} N... 100644 100644 100644 100644 1111111 2222222 3333333 {path}")
    }

    #[test]
    fn parses_both_modified_and_extracts_path() {
        let out = parse_conflicts(&u("UU", "src/app.rs"));
        assert_eq!(out, vec![Conflict { path: "src/app.rs".into(), kind: ConflictKind::BothModified }]);
    }

    #[test]
    fn decodes_every_unmerged_code() {
        for (code, kind) in [
            ("UU", ConflictKind::BothModified),
            ("AA", ConflictKind::BothAdded),
            ("DD", ConflictKind::BothDeleted),
            ("AU", ConflictKind::AddedByUs),
            ("UA", ConflictKind::AddedByThem),
            ("DU", ConflictKind::DeletedByUs),
            ("UD", ConflictKind::DeletedByThem),
        ] {
            assert_eq!(parse_conflicts(&u(code, "f"))[0].kind, kind, "code {code}");
        }
    }

    #[test]
    fn preserves_a_path_with_spaces() {
        let out = parse_conflicts(&u("UU", "my dir/a file.txt"));
        assert_eq!(out[0].path, "my dir/a file.txt");
    }

    #[test]
    fn ignores_non_unmerged_lines() {
        let mixed = format!(
            "# branch.head main\n1 .M N... 100644 100644 100644 aaa bbb tracked.rs\n{}\n? untracked.txt",
            u("UU", "conflicted.rs")
        );
        let out = parse_conflicts(&mixed);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, "conflicted.rs");
    }

    #[test]
    fn empty_and_garbage_yield_nothing_without_panic() {
        assert!(parse_conflicts("").is_empty());
        assert!(parse_conflicts("u\nu bad\nnot a status line").is_empty());
    }

    #[test]
    fn unknown_code_maps_to_unknown() {
        assert_eq!(parse_conflicts(&u("XY", "f"))[0].kind, ConflictKind::Unknown);
    }
}
