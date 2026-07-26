//! Revert attribution (CPE-1079, epic CPE-732): fold the audit journal into the set of root-relative
//! paths a given agent `session` MUTATED at/after a checkpoint timestamp — the foundation the
//! revert-safety classifier uses to tell "the agent changed this" from "someone else did".
//!
//! **Pure fold** over `audit_journal::AuditEvent`. Std-only, no new deps, does not modify
//! `audit_journal`. Paths are treated as opaque strings normalized only by `/`-segment splitting (no
//! `std::path`, no `#[cfg]`), so behavior is identical on all three CI OSes.
//!
//! ## Rename detail format (shared with `replay.rs`)
//! A `renamed` event's target path is parsed out of `detail` using the same `"-> <target>"` convention
//! `replay.rs` documents and tests against (`RENAME_TARGET_MARKER`). This module intentionally mirrors
//! that parsing rather than importing it, since `replay`'s helper is private — if the convention ever
//! changes, both modules must change together (see `replay.rs` module docs for the authoritative
//! description of the format).

use crate::audit_journal::AuditEvent;
use std::collections::BTreeSet;

/// Marker prefix a `renamed` event's `detail` is expected to carry ahead of the target path. Mirrors
/// `replay::RENAME_TARGET_MARKER`.
const RENAME_TARGET_MARKER: &str = "-> ";

/// Root-relative `/`-segment keys of every path `session` MUTATED at/after `since_ts`.
///
/// Keeps events where `session` matches, `ts >= since_ts`, and `kind` is one of `created` / `modified`
/// / `removed` / `renamed` (a `read` is an access, not a mutation, and is excluded). Each surviving
/// event's `path` is mapped to a root-relative key via [`to_root_relative`]; an event whose path isn't
/// under `root` is skipped entirely (not an error). A `renamed` event additionally contributes its
/// rename target, parsed out of `detail` by the `"-> <target>"` convention `replay.rs` uses — an
/// unparseable target skips just the target (the source path is still recorded).
///
/// An empty or non-matching event stream yields an empty set.
pub fn agent_touched(
    events: &[AuditEvent],
    session: &str,
    since_ts: u64,
    root: &str,
) -> BTreeSet<String> {
    let mut touched = BTreeSet::new();
    for e in events {
        if e.session != session || e.ts < since_ts || !is_mutating(&e.kind) {
            continue;
        }
        if let Some(key) = to_root_relative(root, &e.path) {
            touched.insert(key);
        }
        if e.kind == "renamed" {
            if let Some(target) = rename_target(e) {
                if let Some(key) = to_root_relative(root, &target) {
                    touched.insert(key);
                }
            }
        }
    }
    touched
}

/// Whether an event `kind` counts as a mutation for attribution purposes (excludes `read`).
fn is_mutating(kind: &str) -> bool {
    matches!(kind, "created" | "modified" | "removed" | "renamed")
}

/// Parse a `renamed` event's target path out of `detail`, per the `"-> <target>"` convention. `None`
/// if `detail` is absent, doesn't carry the marker, or the target is empty.
fn rename_target(e: &AuditEvent) -> Option<String> {
    let detail = e.detail.as_deref()?;
    let target = detail.strip_prefix(RENAME_TARGET_MARKER)?;
    if target.is_empty() {
        None
    } else {
        Some(target.to_string())
    }
}

/// Cross-OS containment: `Some(root-relative key)` if `abs` is under `root` by **`/`-segment
/// equality**, else `None`. `\` is normalized to `/` before splitting; empty segments (leading /
/// trailing / doubled slashes) are dropped from both sides. Containment requires every one of
/// `root`'s segments to be a segment-wise prefix of `abs`'s — never a raw string `starts_with`, so
/// `root=/repo` does NOT treat `/repo-secrets/x` as contained (only a true path-segment prefix match
/// counts). The returned key is the remaining segments joined by `/`; a `root == abs` match yields
/// `Some(String::new())`.
pub fn to_root_relative(root: &str, abs: &str) -> Option<String> {
    let root_segs: Vec<&str> = split_segments(root);
    let abs_segs: Vec<&str> = split_segments(abs);
    if abs_segs.len() < root_segs.len() {
        return None;
    }
    if root_segs.iter().zip(abs_segs.iter()).any(|(r, a)| r != a) {
        return None;
    }
    Some(abs_segs[root_segs.len()..].join("/"))
}

/// Split a path-like string into non-empty `/`-segments, normalizing `\` to `/` first.
fn split_segments(s: &str) -> Vec<&str> {
    // `\` is normalized by treating it as a segment separator alongside `/`, without allocating a
    // new String: split on both characters directly.
    s.split(['/', '\\']).filter(|seg| !seg.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(ts: u64, session: &str, kind: &str, path: &str) -> AuditEvent {
        AuditEvent { ts, session: session.into(), kind: kind.into(), path: path.into(), detail: None }
    }

    fn renamed(ts: u64, session: &str, from: &str, to: &str) -> AuditEvent {
        AuditEvent {
            ts,
            session: session.into(),
            kind: "renamed".into(),
            path: from.into(),
            detail: Some(format!("-> {to}")),
        }
    }

    #[test]
    fn empty_events_yield_empty_set() {
        assert!(agent_touched(&[], "s1", 0, "/repo").is_empty());
    }

    #[test]
    fn read_is_excluded_mutating_kinds_included() {
        let events = [
            ev(10, "s1", "read", "/repo/a.txt"),
            ev(10, "s1", "created", "/repo/b.txt"),
            ev(10, "s1", "modified", "/repo/c.txt"),
            ev(10, "s1", "removed", "/repo/d.txt"),
        ];
        let got = agent_touched(&events, "s1", 0, "/repo");
        assert_eq!(
            got,
            BTreeSet::from(["b.txt".to_string(), "c.txt".to_string(), "d.txt".to_string()])
        );
    }

    #[test]
    fn ts_before_checkpoint_and_other_session_excluded() {
        let events = [
            ev(5, "s1", "modified", "/repo/old.txt"), // ts < since_ts
            ev(10, "s2", "modified", "/repo/other.txt"), // wrong session
            ev(10, "s1", "modified", "/repo/keep.txt"), // matches
        ];
        let got = agent_touched(&events, "s1", 10, "/repo");
        assert_eq!(got, BTreeSet::from(["keep.txt".to_string()]));
    }

    #[test]
    fn backslash_and_slash_input_map_to_same_key() {
        let events = [
            ev(10, "s1", "modified", "/repo/sub/a.txt"),
            ev(10, "s1", "modified", r"\repo\sub\a.txt"),
        ];
        let got = agent_touched(&events, "s1", 0, "/repo");
        assert_eq!(got, BTreeSet::from(["sub/a.txt".to_string()]));

        let got_win_root = agent_touched(&events, "s1", 0, r"\repo");
        assert_eq!(got_win_root, BTreeSet::from(["sub/a.txt".to_string()]));
    }

    #[test]
    fn prefix_collision_guard() {
        // root=/repo must NOT treat /repo-secrets/x as contained (segment equality, not starts_with).
        let events = [
            ev(10, "s1", "modified", "/repo-secrets/x"),
            ev(10, "s1", "modified", "/repo/x"),
        ];
        let got = agent_touched(&events, "s1", 0, "/repo");
        assert_eq!(got, BTreeSet::from(["x".to_string()]));
    }

    #[test]
    fn renamed_yields_source_and_target() {
        let events = [renamed(10, "s1", "/repo/old.txt", "/repo/new.txt")];
        let got = agent_touched(&events, "s1", 0, "/repo");
        assert_eq!(
            got,
            BTreeSet::from(["old.txt".to_string(), "new.txt".to_string()])
        );
    }

    #[test]
    fn renamed_with_unparseable_target_skips_just_the_target() {
        let no_marker = AuditEvent {
            ts: 10,
            session: "s1".into(),
            kind: "renamed".into(),
            path: "/repo/old.txt".into(),
            detail: Some("nonsense".into()),
        };
        let no_detail = AuditEvent {
            ts: 10,
            session: "s1".into(),
            kind: "renamed".into(),
            path: "/repo/old2.txt".into(),
            detail: None,
        };
        let got = agent_touched(&[no_marker, no_detail], "s1", 0, "/repo");
        assert_eq!(got, BTreeSet::from(["old.txt".to_string(), "old2.txt".to_string()]));
    }

    #[test]
    fn out_of_root_path_is_skipped_without_panic() {
        let events = [ev(10, "s1", "modified", "/elsewhere/x.txt")];
        assert!(agent_touched(&events, "s1", 0, "/repo").is_empty());
    }

    #[test]
    fn renamed_target_out_of_root_skips_only_target() {
        let events = [renamed(10, "s1", "/repo/old.txt", "/elsewhere/new.txt")];
        let got = agent_touched(&events, "s1", 0, "/repo");
        assert_eq!(got, BTreeSet::from(["old.txt".to_string()]));
    }

    #[test]
    fn to_root_relative_exact_root_match_yields_empty_key() {
        assert_eq!(to_root_relative("/repo", "/repo"), Some(String::new()));
    }

    #[test]
    fn to_root_relative_none_when_shorter_than_root() {
        assert_eq!(to_root_relative("/repo/sub", "/repo"), None);
    }

    #[test]
    fn to_root_relative_none_when_not_under_root() {
        assert_eq!(to_root_relative("/repo", "/other/x"), None);
    }
}
