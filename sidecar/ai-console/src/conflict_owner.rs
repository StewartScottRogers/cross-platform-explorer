//! Per-file attribution + deterministic owner (CPE-1069, epic CPE-730): given each agent/session's
//! filesystem activity, fold it into a per-path contributor feed (who touched this file, and how
//! much) plus a deterministic `owner` for heat-map colouring. Pure fold over
//! [`conflict::AgentActivity`] — no new dependency, no recursion, no filesystem access. Distinct
//! from `conflict.rs`'s pairwise contention detection: this module answers "who else is here" and
//! "whose colour is this file" rather than "is this file contended".
//!
//! Scope of this slice: edits/deletes only (mirrors `AgentActivity`'s two path sets). Renames are
//! out of scope, same as `conflict.rs`.

use std::collections::BTreeMap;

use crate::conflict::AgentActivity;

/// One path's contributor feed and its deterministic owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathAttribution {
    pub path: String,
    /// `(agent, edits, deletes)`, sorted by agent.
    pub contributors: Vec<(String, u32, u32)>,
    /// The agent that "owns" this path for heat-map colouring (see [`attribute`] for the rule).
    pub owner: String,
}

/// Fold `activity` into a per-path contributor feed plus a deterministic owner.
///
/// Owner rule, applied per path over that path's contributors: most edits wins; ties broken by
/// most deletes; remaining ties broken by the lexically-least agent name. This makes heat-map
/// colouring stable and reproducible across runs regardless of input order.
///
/// `contributors` is sorted by agent name; the returned `Vec<PathAttribution>` is sorted by path.
/// Counts accumulate with `u32::saturating_add` so a pathological stream can't overflow/panic.
/// Empty input yields empty output. Pure — no recursion, no I/O.
pub fn attribute(activity: &[AgentActivity]) -> Vec<PathAttribution> {
    // path -> agent -> (edits, deletes)
    let mut per_path: BTreeMap<&str, BTreeMap<&str, (u32, u32)>> = BTreeMap::new();

    for a in activity {
        for p in &a.edited {
            let counts = per_path.entry(p).or_default().entry(&a.agent).or_insert((0, 0));
            counts.0 = counts.0.saturating_add(1);
        }
        for p in &a.deleted {
            let counts = per_path.entry(p).or_default().entry(&a.agent).or_insert((0, 0));
            counts.1 = counts.1.saturating_add(1);
        }
    }

    let mut out = Vec::with_capacity(per_path.len());
    for (path, by_agent) in per_path {
        let contributors: Vec<(String, u32, u32)> = by_agent
            .iter()
            .map(|(agent, (edits, deletes))| (agent.to_string(), *edits, *deletes))
            .collect();

        // Owner: most edits -> most deletes -> lexically-least agent. BTreeMap iteration is
        // already agent-sorted, so a stable min_by_key with these keys resolves ties correctly:
        // pick the candidate that minimizes (-edits, -deletes, agent), i.e. maximizes
        // (edits, deletes) and, on a full tie, the smallest agent name.
        let owner = by_agent
            .iter()
            .min_by_key(|(agent, (edits, deletes))| (std::cmp::Reverse(*edits), std::cmp::Reverse(*deletes), **agent))
            .map(|(agent, _)| agent.to_string())
            .unwrap_or_default();

        out.push(PathAttribution { path: path.to_string(), contributors, owner });
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
    fn empty_input_yields_empty_output() {
        assert!(attribute(&[]).is_empty());
    }

    #[test]
    fn single_agent_path_is_owned_by_that_agent() {
        let a = [agent("alice", &["src/a.rs"], &[])];
        let out = attribute(&a);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, "src/a.rs");
        assert_eq!(out[0].owner, "alice");
        assert_eq!(out[0].contributors, vec![("alice".to_string(), 1, 0)]);
    }

    #[test]
    fn contested_path_owner_is_top_editor() {
        // `edited` is a BTreeSet, so duplicate paths within one AgentActivity collapse to 1;
        // to give alice 2 edits on the same path, split across two activity entries (e.g. two
        // ticks/sessions from the same agent).
        let a = [
            agent("alice", &["shared.rs"], &[]),
            agent("alice", &["shared.rs"], &[]),
            agent("bob", &["shared.rs"], &[]),
        ];
        let out = attribute(&a);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].owner, "alice");
        assert_eq!(
            out[0].contributors,
            vec![("alice".to_string(), 2, 0), ("bob".to_string(), 1, 0)]
        );
    }

    #[test]
    fn tie_on_edits_broken_by_deletes() {
        let a = [
            agent("alice", &["hot.rs"], &["hot.rs"]),
            agent("bob", &["hot.rs"], &[]),
        ];
        // Both have 1 edit; alice also has 1 delete (same-agent edit+delete is legal per AgentActivity).
        let out = attribute(&a);
        assert_eq!(out[0].owner, "alice");
    }

    #[test]
    fn full_tie_breaks_lexically_least_agent() {
        let a = [agent("zoe", &["f.rs"], &[]), agent("amy", &["f.rs"], &[])];
        let out = attribute(&a);
        assert_eq!(out[0].owner, "amy");
        // contributors sorted by agent too
        assert_eq!(
            out[0].contributors,
            vec![("amy".to_string(), 1, 0), ("zoe".to_string(), 1, 0)]
        );
    }

    #[test]
    fn results_sorted_by_path() {
        let a = [agent("alice", &["z.rs", "a.rs", "m.rs"], &[])];
        let out = attribute(&a);
        let paths: Vec<&str> = out.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, vec!["a.rs", "m.rs", "z.rs"]);
    }

    #[test]
    fn deletes_count_toward_contributors_without_edits() {
        let a = [agent("deleter", &[], &["gone.rs"])];
        let out = attribute(&a);
        assert_eq!(out[0].contributors, vec![("deleter".to_string(), 0, 1)]);
        assert_eq!(out[0].owner, "deleter");
    }

    #[test]
    fn saturating_add_used_for_repeated_edits_across_activity_entries() {
        // Simulate many activity entries from the same agent on the same path; counts must not
        // overflow (they saturate at u32::MAX rather than panicking or wrapping).
        let many: Vec<AgentActivity> = (0..5).map(|_| agent("alice", &["hammered.rs"], &[])).collect();
        let out = attribute(&many);
        assert_eq!(out[0].contributors, vec![("alice".to_string(), 5, 0)]);
    }
}
