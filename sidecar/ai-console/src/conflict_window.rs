//! Temporal contention window (CPE-1068, epic CPE-730): given a stream of per-path touch
//! events, flag a path where two *different* agents touched it within a short time window.
//! `conflict.rs` answers "did two agents touch the same path at all" (time-agnostic); this
//! module answers "did they touch it close enough in time to actually be live contention."
//! Pure fold over already-attributed events — no filesystem, no clock, no deps.

/// One agent/session touching one path at one point in time (milliseconds, any epoch —
/// the caller's clock source doesn't matter, only relative gaps do).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TouchEvent {
    pub agent: String,
    pub path: String,
    pub ts_ms: u64,
}

/// A path where two or more distinct agents touched it within the contention window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowConflict {
    pub path: String,
    /// Distinct contending agents, sorted.
    pub agents: Vec<String>,
}

/// Flag paths touched by two different agents with a time gap `<= window_ms`.
///
/// Groups `events` by `path`, sorts each group by `ts_ms`, then walks adjacent pairs in
/// that sorted order: if two consecutive touches are by different agents and their gap is
/// within the window, both agents are recorded as contending on that path. Time-gap math
/// uses `u64::saturating_sub` in both directions so out-of-order timestamps (or `u64::MAX`
/// values) can never underflow/panic — a reversed pair just saturates to `0`, which is
/// always `<= window_ms` and correctly still flags contention.
///
/// Results are sorted by path; each path's `agents` list is sorted and deduplicated.
pub fn contended_within(events: &[TouchEvent], window_ms: u64) -> Vec<WindowConflict> {
    let mut by_path: std::collections::BTreeMap<&str, Vec<&TouchEvent>> =
        std::collections::BTreeMap::new();
    for e in events {
        by_path.entry(e.path.as_str()).or_default().push(e);
    }

    let mut out = Vec::new();
    for (path, mut touches) in by_path {
        touches.sort_by_key(|e| e.ts_ms);

        let mut contenders: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for pair in touches.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a.agent == b.agent {
                continue;
            }
            let gap = a.ts_ms.saturating_sub(b.ts_ms).max(b.ts_ms.saturating_sub(a.ts_ms));
            if gap <= window_ms {
                contenders.insert(a.agent.as_str());
                contenders.insert(b.agent.as_str());
            }
        }

        if !contenders.is_empty() {
            out.push(WindowConflict {
                path: path.to_string(),
                agents: contenders.into_iter().map(str::to_string).collect(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(agent: &str, path: &str, ts_ms: u64) -> TouchEvent {
        TouchEvent { agent: agent.to_string(), path: path.to_string(), ts_ms }
    }

    #[test]
    fn two_different_agents_inside_window_are_flagged_sorted() {
        let events = [touch("bob", "shared.rs", 1_000), touch("alice", "shared.rs", 1_050)];
        let c = contended_within(&events, 100);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].path, "shared.rs");
        assert_eq!(c[0].agents, vec!["alice", "bob"]); // sorted, not insertion order
    }

    #[test]
    fn just_outside_window_is_not_flagged() {
        let events = [touch("alice", "shared.rs", 1_000), touch("bob", "shared.rs", 1_101)];
        let c = contended_within(&events, 100);
        assert!(c.is_empty());
    }

    #[test]
    fn exactly_at_window_boundary_is_flagged() {
        let events = [touch("alice", "shared.rs", 1_000), touch("bob", "shared.rs", 1_100)];
        let c = contended_within(&events, 100);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn same_agent_rapid_touches_are_not_flagged() {
        let events = [touch("alice", "solo.rs", 1_000), touch("alice", "solo.rs", 1_001)];
        let c = contended_within(&events, 100);
        assert!(c.is_empty());
    }

    #[test]
    fn out_of_order_input_is_handled_by_internal_sort() {
        // Later timestamp appears first in the input slice.
        let events = [touch("bob", "shared.rs", 1_100), touch("alice", "shared.rs", 1_050)];
        let c = contended_within(&events, 100);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].agents, vec!["alice", "bob"]);
    }

    #[test]
    fn single_event_is_empty() {
        let events = [touch("alice", "only.rs", 1_000)];
        assert!(contended_within(&events, 100).is_empty());
    }

    #[test]
    fn results_sorted_by_path() {
        let events = [
            touch("a", "z.rs", 0),
            touch("b", "z.rs", 1),
            touch("a", "a.rs", 0),
            touch("b", "a.rs", 1),
        ];
        let c = contended_within(&events, 10);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].path, "a.rs");
        assert_eq!(c[1].path, "z.rs");
    }

    #[test]
    fn three_agents_on_one_path_collects_all_contending_pairs() {
        let events = [
            touch("alice", "shared.rs", 0),
            touch("bob", "shared.rs", 10),
            touch("carol", "shared.rs", 20),
        ];
        let c = contended_within(&events, 15);
        assert_eq!(c.len(), 1);
        // alice-bob gap 10 (in), bob-carol gap 10 (in) -> all three contend transitively
        // via adjacency, even though alice-carol gap is 20 (out of window on its own).
        assert_eq!(c[0].agents, vec!["alice", "bob", "carol"]);
    }

    #[test]
    fn u64_max_timestamps_and_window_do_not_panic() {
        let events = [
            touch("alice", "edge.rs", 0),
            touch("bob", "edge.rs", u64::MAX),
        ];
        // window_ms == u64::MAX: gap saturates but the comparison must not panic.
        let c = contended_within(&events, u64::MAX);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].agents, vec!["alice", "bob"]);

        // Small window: gap is huge, so no contention, but still must not panic.
        let c2 = contended_within(&events, 0);
        assert!(c2.is_empty());
    }

    #[test]
    fn fuzzy_zero_and_max_mix_returns_cleanly() {
        // A jumbled [0, u64::MAX] mix, out of order, across several paths and agents —
        // must not panic/wrap for any window value, including window_ms == u64::MAX.
        let events = [
            touch("a", "p.rs", u64::MAX),
            touch("b", "p.rs", 0),
            touch("c", "q.rs", u64::MAX),
            touch("d", "q.rs", u64::MAX),
            touch("e", "r.rs", 0),
            touch("f", "r.rs", u64::MAX),
        ];
        for window in [0, 1, 5, u64::MAX / 2, u64::MAX - 1, u64::MAX] {
            let _ = contended_within(&events, window);
        }

        // A tight window shouldn't flag paths whose only gap is the full 0..MAX span.
        let c = contended_within(&events, 5);
        assert!(
            c.iter().all(|w| w.path != "p.rs"),
            "0 and u64::MAX are far apart, gap 5 should not flag p.rs"
        );
    }

    #[test]
    fn empty_events_is_empty() {
        let events: [TouchEvent; 0] = [];
        assert!(contended_within(&events, 1_000).is_empty());
    }
}
