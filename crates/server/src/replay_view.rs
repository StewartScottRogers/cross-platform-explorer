//! Replay folder-view projection + transition diff (CPE-1065, epic CPE-728): given a reconstructed
//! [`crate::replay::FsState`] (see `replay::state_at`), answer "what direct children does this folder
//! show at moment T" and "what changed between two scrub cursors" (to animate a scrub).
//!
//! **Pure projection over `FsState`.** Std-only, no new deps, does not touch `replay.rs` — it only reads
//! the `FsState`/`FsNode` types that module already exposes. Paths are treated as opaque strings and
//! split on normalized `/` segments (never `std::path::Path::parent()`/`components()`, whose separator
//! semantics differ across platforms), so both functions behave identically on all three CI OSes.

use crate::replay::FsState;

/// One direct child of a folder in a replay projection, ready to render as a folder-view row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReplayEntry {
    /// The child's own name (its final path segment).
    pub name: String,
    /// The child's full path, as stored in `FsState`.
    pub path: String,
    /// Timestamp (epoch ms) of the last event that touched this path.
    pub ts: u64,
    /// The kind of the last event that touched this path (`created`/`modified`/`renamed`, ...).
    pub kind: String,
}

/// What changed between two [`FsState`] snapshots, e.g. to animate a scrub from `a` to `b`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StateDiff {
    /// Paths present in `b` but not `a`.
    pub added: Vec<String>,
    /// Paths present in `a` but not `b`.
    pub removed: Vec<String>,
    /// Paths present in both, whose node (`ts`/`kind`) differs.
    pub modified: Vec<String>,
}

/// Split `path` into its non-empty `/`-separated segments, normalizing `\` to `/` first so the split
/// behaves identically regardless of which separator produced the string. Purely string-based — no
/// `std::path` platform semantics.
fn segments(path: &str) -> Vec<String> {
    path.replace('\\', "/").split('/').filter(|s| !s.is_empty()).map(str::to_string).collect()
}

/// The direct children (exactly one level below `dir`) present in `state`. A nested grandchild (e.g.
/// `a/b/c` is not a child of `a`) is excluded. `dir` may be the root (empty string, `/`, or any string
/// with zero non-empty segments), in which case top-level entries are returned. Output is deterministic:
/// `state` is a `BTreeMap`, so entries come out in ascending path order.
pub fn children_at(state: &FsState, dir: &str) -> Vec<ReplayEntry> {
    let dir_segs = segments(dir);
    let mut out = Vec::new();
    for (path, node) in state.iter() {
        let segs = segments(path);
        if segs.len() != dir_segs.len() + 1 {
            continue;
        }
        if segs[..dir_segs.len()] != dir_segs[..] {
            continue;
        }
        out.push(ReplayEntry {
            name: segs[dir_segs.len()].clone(),
            path: path.clone(),
            ts: node.ts,
            kind: node.kind.clone(),
        });
    }
    out
}

/// Classify what changed from state `a` to state `b`: `added` (in `b` not `a`), `removed` (in `a` not
/// `b`), `modified` (in both but the node's `ts`/`kind` differs). Identical states yield an empty diff.
/// Deterministic: both input maps are `BTreeMap`s, so each output vec comes out in ascending path order.
pub fn diff_states(a: &FsState, b: &FsState) -> StateDiff {
    let mut diff = StateDiff::default();
    for (path, b_node) in b.iter() {
        match a.get(path) {
            None => diff.added.push(path.clone()),
            Some(a_node) => {
                if a_node != b_node {
                    diff.modified.push(path.clone());
                }
            }
        }
    }
    for path in a.keys() {
        if !b.contains_key(path) {
            diff.removed.push(path.clone());
        }
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::FsNode;

    fn node(ts: u64, kind: &str) -> FsNode {
        FsNode { ts, kind: kind.to_string() }
    }

    fn state(entries: &[(&str, u64, &str)]) -> FsState {
        entries.iter().map(|(p, ts, kind)| (p.to_string(), node(*ts, kind))).collect()
    }

    // --- children_at ---

    #[test]
    fn children_at_root_returns_only_top_level() {
        let s = state(&[("/a", 1, "created"), ("/b", 2, "created"), ("/a/nested", 3, "created")]);
        let mut kids = children_at(&s, "/");
        kids.sort_by(|x, y| x.path.cmp(&y.path));
        let names: Vec<_> = kids.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn children_at_root_with_empty_string_dir_is_equivalent() {
        let s = state(&[("/a", 1, "created"), ("/a/b", 2, "created")]);
        let via_slash = children_at(&s, "/");
        let via_empty = children_at(&s, "");
        assert_eq!(via_slash, via_empty);
    }

    #[test]
    fn children_at_nested_dir_excludes_grandchildren() {
        let s = state(&[
            ("/a", 1, "created"),
            ("/a/b", 2, "created"),
            ("/a/b/c.txt", 3, "created"),
            ("/a/other", 4, "created"),
        ]);
        let kids = children_at(&s, "/a");
        let names: std::collections::BTreeSet<_> = kids.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, ["b", "other"].into_iter().map(String::from).collect());
        // The grandchild must not appear under any name.
        assert!(kids.iter().all(|e| e.path != "/a/b/c.txt"));
    }

    #[test]
    fn children_at_dir_with_trailing_slash_matches_without() {
        let s = state(&[("/a/b", 1, "created")]);
        assert_eq!(children_at(&s, "/a"), children_at(&s, "/a/"));
    }

    #[test]
    fn children_at_is_deterministically_sorted() {
        let s = state(&[("/z", 1, "created"), ("/a", 2, "created"), ("/m", 3, "created")]);
        let kids = children_at(&s, "/");
        let paths: Vec<_> = kids.iter().map(|e| e.path.clone()).collect();
        assert_eq!(paths, vec!["/a".to_string(), "/m".to_string(), "/z".to_string()]);
    }

    #[test]
    fn children_at_returns_entry_fields_from_node() {
        let s = state(&[("/a", 42, "modified")]);
        let kids = children_at(&s, "/");
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].name, "a");
        assert_eq!(kids[0].path, "/a");
        assert_eq!(kids[0].ts, 42);
        assert_eq!(kids[0].kind, "modified");
    }

    #[test]
    fn children_at_empty_state_is_empty() {
        let s = FsState::new();
        assert!(children_at(&s, "/").is_empty());
        assert!(children_at(&s, "/a").is_empty());
    }

    #[test]
    fn children_at_no_matches_under_unrelated_dir_is_empty() {
        let s = state(&[("/a/b", 1, "created")]);
        assert!(children_at(&s, "/x").is_empty());
    }

    #[test]
    fn children_at_backslash_input_is_normalized() {
        // Defensive: even though FsState paths are documented as already-normalized, children_at
        // must not rely on std::path platform semantics — verify a stray backslash still splits
        // as a `/`-segment would, on every OS (not gated behind #[cfg(windows)]).
        let s = state(&[("a/b", 1, "created")]);
        assert_eq!(children_at(&s, "a\\"), children_at(&s, "a/"));
    }

    // --- diff_states ---

    #[test]
    fn diff_states_identical_states_is_empty() {
        let s = state(&[("/a", 1, "created"), ("/b", 2, "created")]);
        assert_eq!(diff_states(&s, &s), StateDiff::default());
    }

    #[test]
    fn diff_states_detects_added() {
        let a = state(&[("/a", 1, "created")]);
        let b = state(&[("/a", 1, "created"), ("/b", 2, "created")]);
        let diff = diff_states(&a, &b);
        assert_eq!(diff.added, vec!["/b".to_string()]);
        assert!(diff.removed.is_empty());
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn diff_states_detects_removed() {
        let a = state(&[("/a", 1, "created"), ("/b", 2, "created")]);
        let b = state(&[("/a", 1, "created")]);
        let diff = diff_states(&a, &b);
        assert_eq!(diff.removed, vec!["/b".to_string()]);
        assert!(diff.added.is_empty());
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn diff_states_detects_modified_by_ts() {
        let a = state(&[("/a", 1, "created")]);
        let b = state(&[("/a", 2, "created")]);
        let diff = diff_states(&a, &b);
        assert_eq!(diff.modified, vec!["/a".to_string()]);
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn diff_states_detects_modified_by_kind() {
        let a = state(&[("/a", 1, "created")]);
        let b = state(&[("/a", 1, "modified")]);
        let diff = diff_states(&a, &b);
        assert_eq!(diff.modified, vec!["/a".to_string()]);
    }

    #[test]
    fn diff_states_unchanged_entry_is_not_modified() {
        let a = state(&[("/a", 1, "created")]);
        let b = state(&[("/a", 1, "created")]);
        assert_eq!(diff_states(&a, &b), StateDiff::default());
    }

    #[test]
    fn diff_states_mixed_added_removed_modified() {
        let a = state(&[("/a", 1, "created"), ("/b", 2, "created"), ("/c", 3, "created")]);
        let b = state(&[("/a", 1, "created"), ("/b", 99, "modified"), ("/d", 4, "created")]);
        let diff = diff_states(&a, &b);
        assert_eq!(diff.added, vec!["/d".to_string()]);
        assert_eq!(diff.removed, vec!["/c".to_string()]);
        assert_eq!(diff.modified, vec!["/b".to_string()]);
    }

    #[test]
    fn diff_states_both_empty_is_empty() {
        assert_eq!(diff_states(&FsState::new(), &FsState::new()), StateDiff::default());
    }

    #[test]
    fn diff_states_from_empty_is_all_added() {
        let a = FsState::new();
        let b = state(&[("/a", 1, "created"), ("/b", 2, "created")]);
        let diff = diff_states(&a, &b);
        assert_eq!(diff.added, vec!["/a".to_string(), "/b".to_string()]);
        assert!(diff.removed.is_empty());
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn diff_states_to_empty_is_all_removed() {
        let a = state(&[("/a", 1, "created"), ("/b", 2, "created")]);
        let b = FsState::new();
        let diff = diff_states(&a, &b);
        assert_eq!(diff.removed, vec!["/a".to_string(), "/b".to_string()]);
        assert!(diff.added.is_empty());
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn diff_states_ordering_is_deterministic() {
        let a = state(&[("/z", 1, "created")]);
        let b = state(&[("/z", 1, "created"), ("/a", 1, "created"), ("/m", 1, "created")]);
        let diff = diff_states(&a, &b);
        // Added entries come out sorted (BTreeMap iteration order), not insertion order.
        assert_eq!(diff.added, vec!["/a".to_string(), "/m".to_string()]);
    }

    // --- integration with replay::state_at ---

    #[test]
    fn children_at_and_diff_states_work_over_state_at_output() {
        use crate::audit_journal::AuditEvent;
        use crate::replay::state_at;

        fn ev(ts: u64, kind: &str, path: &str) -> AuditEvent {
            AuditEvent { ts, session: "s1".into(), kind: kind.into(), path: path.into(), detail: None }
        }

        let events = [
            ev(10, "created", "/a"),
            ev(20, "created", "/a/b"),
            ev(30, "created", "/a/b/c.txt"),
        ];
        let before = state_at(&events, 15);
        let after = state_at(&events, 30);

        // At t=15, "/a" has no visible children yet.
        assert!(children_at(&before, "/a").is_empty());
        // At t=30, "/a" has exactly one direct child, "/a/b" (the grandchild is excluded).
        let kids = children_at(&after, "/a");
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].path, "/a/b");

        let diff = diff_states(&before, &after);
        assert_eq!(diff.added, vec!["/a/b".to_string(), "/a/b/c.txt".to_string()]);
        assert!(diff.removed.is_empty());
        assert!(diff.modified.is_empty());
    }
}
