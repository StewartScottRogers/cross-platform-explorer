//! Competing-rename detection (CPE-1067, epic CPE-730): given each agent/session's rename
//! activity, find where two agents' renames contend — the same source renamed to different
//! targets (divergence), or different sources renamed onto the same target (collision). Pure
//! set logic over already-attributed activity, mirroring the style of `conflict.rs` (CPE-914),
//! which covers edit-edit and edit-delete contention; this module covers the rename case that
//! ticket documented as a follow-up.

use std::collections::{BTreeMap, BTreeSet};

/// One agent/session's rename activity on the shared tree: a list of `(from, to)` pairs.
#[derive(Debug, Clone, Default)]
pub struct RenameActivity {
    pub agent: String,
    pub renames: Vec<(String, String)>,
}

impl RenameActivity {
    pub fn new(agent: impl Into<String>) -> Self {
        Self { agent: agent.into(), renames: Vec::new() }
    }
}

/// What kind of rename contention a path is under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameConflictKind {
    /// Two or more agents renamed the same source path to different targets.
    Divergence,
    /// Two or more agents renamed different source paths onto the same target.
    Collision,
}

/// A detected rename conflict on one path, and the agents involved (sorted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameConflict {
    pub path: String,
    pub kind: RenameConflictKind,
    pub agents: Vec<String>,
}

/// Detect competing renames across `activity`.
///
/// - **Divergence**: 2+ distinct agents rename the same `from` to different `to` paths →
///   conflict keyed on `from`.
/// - **Collision**: 2+ distinct agents rename different `from` paths onto the same `to` →
///   conflict keyed on `to`.
///
/// Same-agent repeated renames of a path are not a conflict (only distinct agents count), and a
/// `from == to` no-op rename is ignored entirely. Results are sorted by path, with each
/// conflict's agents sorted, for a stable radar order (mirrors `conflict.rs`).
pub fn detect_rename_conflicts(activity: &[RenameActivity]) -> Vec<RenameConflict> {
    // Keyed by `from`: distinct (agent, to) pairs seen for that source path.
    let mut by_from: BTreeMap<&str, BTreeSet<(&str, &str)>> = BTreeMap::new();
    // Keyed by `to`: distinct (agent, from) pairs seen for that target path.
    let mut by_to: BTreeMap<&str, BTreeSet<(&str, &str)>> = BTreeMap::new();

    for a in activity {
        for (from, to) in &a.renames {
            if from == to {
                continue; // no-op rename, ignored
            }
            by_from.entry(from.as_str()).or_default().insert((a.agent.as_str(), to.as_str()));
            by_to.entry(to.as_str()).or_default().insert((a.agent.as_str(), from.as_str()));
        }
    }

    let mut out = Vec::new();

    for (from, entries) in &by_from {
        let distinct_agents: BTreeSet<&str> = entries.iter().map(|(agent, _)| *agent).collect();
        let distinct_targets: BTreeSet<&str> = entries.iter().map(|(_, to)| *to).collect();
        if distinct_agents.len() >= 2 && distinct_targets.len() >= 2 {
            out.push(RenameConflict {
                path: from.to_string(),
                kind: RenameConflictKind::Divergence,
                agents: distinct_agents.into_iter().map(str::to_string).collect(),
            });
        }
    }

    for (to, entries) in &by_to {
        let distinct_agents: BTreeSet<&str> = entries.iter().map(|(agent, _)| *agent).collect();
        let distinct_sources: BTreeSet<&str> = entries.iter().map(|(_, from)| *from).collect();
        if distinct_agents.len() >= 2 && distinct_sources.len() >= 2 {
            out.push(RenameConflict {
                path: to.to_string(),
                kind: RenameConflictKind::Collision,
                agents: distinct_agents.into_iter().map(str::to_string).collect(),
            });
        }
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(name: &str, renames: &[(&str, &str)]) -> RenameActivity {
        RenameActivity {
            agent: name.to_string(),
            renames: renames.iter().map(|(f, t)| (f.to_string(), t.to_string())).collect(),
        }
    }

    #[test]
    fn disjoint_renames_have_no_conflicts() {
        let a = [agent("alice", &[("a.rs", "a2.rs")]), agent("bob", &[("b.rs", "b2.rs")])];
        assert!(detect_rename_conflicts(&a).is_empty());
    }

    #[test]
    fn same_from_different_to_is_divergence() {
        let a = [agent("alice", &[("shared.rs", "alice.rs")]), agent("bob", &[("shared.rs", "bob.rs")])];
        let c = detect_rename_conflicts(&a);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].path, "shared.rs");
        assert_eq!(c[0].kind, RenameConflictKind::Divergence);
        assert_eq!(c[0].agents, vec!["alice", "bob"]); // sorted
    }

    #[test]
    fn different_from_same_to_is_collision() {
        let a = [agent("alice", &[("a.rs", "final.rs")]), agent("bob", &[("b.rs", "final.rs")])];
        let c = detect_rename_conflicts(&a);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].path, "final.rs");
        assert_eq!(c[0].kind, RenameConflictKind::Collision);
        assert_eq!(c[0].agents, vec!["alice", "bob"]); // sorted
    }

    #[test]
    fn same_agent_double_rename_is_not_a_conflict() {
        // One agent renaming the same source to two different targets across activity (e.g.
        // retried) is not a conflict — only distinct agents count.
        let a = [agent("solo", &[("shared.rs", "v1.rs"), ("shared.rs", "v2.rs")])];
        assert!(detect_rename_conflicts(&a).is_empty());
    }

    #[test]
    fn same_agent_repeated_target_is_not_a_conflict() {
        let a = [agent("solo", &[("a.rs", "final.rs"), ("b.rs", "final.rs")])];
        assert!(detect_rename_conflicts(&a).is_empty());
    }

    #[test]
    fn from_equals_to_noop_is_ignored() {
        let a = [agent("alice", &[("same.rs", "same.rs")]), agent("bob", &[("same.rs", "same.rs")])];
        assert!(detect_rename_conflicts(&a).is_empty());
    }

    #[test]
    fn from_equals_to_noop_mixed_with_real_rename() {
        // The no-op entry shouldn't suppress or distort a genuine conflict elsewhere.
        let a = [
            agent("alice", &[("noop.rs", "noop.rs"), ("shared.rs", "alice.rs")]),
            agent("bob", &[("shared.rs", "bob.rs")]),
        ];
        let c = detect_rename_conflicts(&a);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].path, "shared.rs");
        assert_eq!(c[0].kind, RenameConflictKind::Divergence);
    }

    #[test]
    fn divergence_needs_two_distinct_targets() {
        // Two agents rename the same source to the SAME target — not a divergence (no
        // disagreement on the destination), and not a collision either (same target, but the
        // sources are identical, not different).
        let a = [agent("alice", &[("shared.rs", "same_target.rs")]), agent("bob", &[("shared.rs", "same_target.rs")])];
        assert!(detect_rename_conflicts(&a).is_empty());
    }

    #[test]
    fn three_agents_divergence_agents_sorted() {
        let a = [
            agent("carol", &[("shared.rs", "c.rs")]),
            agent("alice", &[("shared.rs", "a.rs")]),
            agent("bob", &[("shared.rs", "b.rs")]),
        ];
        let c = detect_rename_conflicts(&a);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].agents, vec!["alice", "bob", "carol"]);
    }

    #[test]
    fn deterministic_sorted_output_by_path() {
        let a = [
            agent("alice", &[("zzz.rs", "z1.rs"), ("aaa.rs", "a1.rs")]),
            agent("bob", &[("zzz.rs", "z2.rs"), ("aaa.rs", "a2.rs")]),
        ];
        let c = detect_rename_conflicts(&a);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].path, "aaa.rs");
        assert_eq!(c[1].path, "zzz.rs");
    }

    #[test]
    fn divergence_and_collision_can_both_be_reported() {
        // alice/bob diverge on "shared.rs"; separately, carol and dave collide onto "final.rs".
        let a = [
            agent("alice", &[("shared.rs", "alice.rs")]),
            agent("bob", &[("shared.rs", "bob.rs")]),
            agent("carol", &[("c.rs", "final.rs")]),
            agent("dave", &[("d.rs", "final.rs")]),
        ];
        let c = detect_rename_conflicts(&a);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].path, "final.rs");
        assert_eq!(c[0].kind, RenameConflictKind::Collision);
        assert_eq!(c[1].path, "shared.rs");
        assert_eq!(c[1].kind, RenameConflictKind::Divergence);
    }
}
