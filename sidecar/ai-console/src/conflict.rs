//! Multi-agent conflict detection (CPE-914, epic CPE-730): given each agent/session's touched paths,
//! find where two agents contend on the same file — both editing it, or one deleting what another edits.
//! Pure set logic over already-attributed activity; the radar UI (banner, per-file "who else is here",
//! owner-coloured heat-map) renders these.
//!
//! Scope of this slice: **edit-edit** and **edit-delete** contention. Competing *renames* are a documented
//! follow-up (they need the rename source→target pairs, a richer activity shape).

use std::collections::{BTreeMap, BTreeSet};

/// One agent/session's filesystem activity on the shared tree.
#[derive(Debug, Clone, Default)]
pub struct AgentActivity {
    pub agent: String,
    /// Paths this agent created/modified.
    pub edited: BTreeSet<String>,
    /// Paths this agent deleted.
    pub deleted: BTreeSet<String>,
}

impl AgentActivity {
    pub fn new(agent: impl Into<String>) -> Self {
        Self { agent: agent.into(), edited: BTreeSet::new(), deleted: BTreeSet::new() }
    }
}

/// What kind of contention a path is under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// Two or more agents edited the same file.
    EditEdit,
    /// One agent deletes a file another is editing — the most destructive case.
    EditDelete,
}

/// A detected conflict on one path, and the agents involved (sorted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub path: String,
    pub kind: ConflictKind,
    pub agents: Vec<String>,
}

/// Detect conflicts across `activity`. A path is a conflict when it's edited by 2+ distinct agents
/// (`EditEdit`), or edited by one agent and deleted by a different one (`EditDelete`, which takes
/// precedence as the more destructive). Same-agent edit+delete of a path is intentional, not a conflict.
/// Results are sorted by path for a stable radar order.
pub fn detect_conflicts(activity: &[AgentActivity]) -> Vec<Conflict> {
    let mut editors: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut deleters: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for a in activity {
        for p in &a.edited {
            editors.entry(p).or_default().insert(&a.agent);
        }
        for p in &a.deleted {
            deleters.entry(p).or_default().insert(&a.agent);
        }
    }

    let paths: BTreeSet<&str> = editors.keys().chain(deleters.keys()).copied().collect();
    let mut out = Vec::new();
    for path in paths {
        let eds = editors.get(path);
        let dels = deleters.get(path);
        let editors_set = eds.cloned().unwrap_or_default();
        let deleters_set = dels.cloned().unwrap_or_default();
        let union: BTreeSet<&str> = editors_set.union(&deleters_set).copied().collect();

        // Edit-delete (more destructive) wins: some agent edits AND a *different* agent deletes.
        if !editors_set.is_empty() && !deleters_set.is_empty() && union.len() >= 2 {
            out.push(Conflict {
                path: path.to_string(),
                kind: ConflictKind::EditDelete,
                agents: union.into_iter().map(str::to_string).collect(),
            });
        } else if editors_set.len() >= 2 {
            out.push(Conflict {
                path: path.to_string(),
                kind: ConflictKind::EditEdit,
                agents: editors_set.into_iter().map(str::to_string).collect(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(name: &str, edited: &[&str], deleted: &[&str]) -> AgentActivity {
        AgentActivity {
            agent: name.to_string(),
            edited: edited.iter().map(|s| s.to_string()).collect(),
            deleted: deleted.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn disjoint_activity_has_no_conflicts() {
        let a = [agent("a", &["src/a.rs"], &[]), agent("b", &["src/b.rs"], &["old.txt"])];
        assert!(detect_conflicts(&a).is_empty());
    }

    #[test]
    fn two_editors_of_one_file_is_edit_edit() {
        let a = [agent("alice", &["shared.rs", "a.rs"], &[]), agent("bob", &["shared.rs"], &[])];
        let c = detect_conflicts(&a);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].path, "shared.rs");
        assert_eq!(c[0].kind, ConflictKind::EditEdit);
        assert_eq!(c[0].agents, vec!["alice", "bob"]);
    }

    #[test]
    fn edit_then_delete_by_another_is_edit_delete() {
        let a = [agent("writer", &["doomed.rs"], &[]), agent("deleter", &[], &["doomed.rs"])];
        let c = detect_conflicts(&a);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].kind, c[0].path.as_str()), (ConflictKind::EditDelete, "doomed.rs"));
        assert_eq!(c[0].agents, vec!["deleter", "writer"]); // sorted
    }

    #[test]
    fn same_agent_edit_and_delete_is_not_a_conflict() {
        let a = [agent("solo", &["temp.rs"], &["temp.rs"])];
        assert!(detect_conflicts(&a).is_empty());
    }

    #[test]
    fn edit_delete_takes_precedence_over_edit_edit() {
        // 2 editors + a 3rd deleter on the same path → report the destructive EditDelete, all 3 involved.
        let a = [
            agent("e1", &["hot.rs"], &[]),
            agent("e2", &["hot.rs"], &[]),
            agent("d1", &[], &["hot.rs"]),
        ];
        let c = detect_conflicts(&a);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].kind, ConflictKind::EditDelete);
        assert_eq!(c[0].agents, vec!["d1", "e1", "e2"]);
    }
}
