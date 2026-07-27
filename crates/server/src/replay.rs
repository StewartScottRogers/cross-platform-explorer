//! Replay projection core (CPE-1063, epic CPE-728): reconstruct the live file-set state at any moment
//! `T` by folding the already-recorded audit-event stream (`audit_journal::AuditEvent`). This is the
//! foundation the scrubbable "activity replay" view sits on — `activity_timeline` buckets events for
//! the minimap, this module answers "what did the tree look like at this instant".
//!
//! **Pure event-sourcing fold.** Std-only, no new deps, does not rebuild `audit_journal` — it only reads
//! the `AuditEvent`s that module already records. Paths are treated as opaque, already-normalized
//! strings: no `std::path` platform semantics, no `#[cfg]`, so the fold behaves identically on all
//! three CI OSes.
//!
//! ## Rename detail format (documented assumption)
//! `audit_journal`'s own test fixture (`detail_round_trips_and_missing_session_is_empty`) encodes a
//! rename's target as `"-> <new path>"` in `detail`. This module follows that convention: a `renamed`
//! event's target path is `detail` with a leading `"-> "` stripped. If `detail` is missing or doesn't
//! start with that marker, the event is treated as a no-op (the entry stays where it is) rather than
//! panicking or guessing — there's no other emitter of `renamed` events yet to confirm a different
//! format against.

use crate::audit_journal::AuditEvent;
use std::collections::BTreeMap;

/// Marker prefix a `renamed` event's `detail` is expected to carry ahead of the target path.
const RENAME_TARGET_MARKER: &str = "-> ";

/// One path's last-known state in a projection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FsNode {
    /// Timestamp (epoch ms) of the last event that touched this path.
    pub ts: u64,
    /// The kind of the last event that touched this path (`created`/`modified`/`renamed`, ...).
    pub kind: String,
}

/// Path → last-touch node. The live set at some moment in time.
pub type FsState = BTreeMap<String, FsNode>;

/// Reconstruct the live file-set state at time `t_ms`: fold every event with `ts <= t_ms`, in `ts`
/// order (events are sorted by `ts` first — stably — so unsorted input still projects correctly).
///
/// Fold rules by `kind`:
/// - `created` — insert the path.
/// - `modified` — update the node's `ts`/`kind` (inserts if the path isn't already present, e.g. the
///   `created` event fell outside the journal's retention window).
/// - `removed` — delete the path.
/// - `renamed` — move the entry from `path` to the target parsed out of `detail` (see module docs). An
///   unparseable `detail` is a no-op.
/// - `read` — no state change (an access, not a mutation).
///
/// An empty event stream yields an empty state.
pub fn state_at(events: &[AuditEvent], t_ms: u64) -> FsState {
    let mut sorted: Vec<&AuditEvent> = events.iter().collect();
    sorted.sort_by_key(|e| e.ts);

    let mut state = FsState::new();
    for e in sorted {
        if e.ts > t_ms {
            break;
        }
        fold(&mut state, e);
    }
    state
}

/// Apply one event to `state` in place, per the fold rules documented on [`state_at`].
fn fold(state: &mut FsState, e: &AuditEvent) {
    match e.kind.as_str() {
        "created" => {
            state.insert(e.path.clone(), FsNode { ts: e.ts, kind: e.kind.clone() });
        }
        "modified" => {
            state.insert(e.path.clone(), FsNode { ts: e.ts, kind: e.kind.clone() });
        }
        "removed" => {
            state.remove(&e.path);
        }
        "renamed" => match rename_target(e) {
            Some(target) => {
                state.remove(&e.path);
                state.insert(target, FsNode { ts: e.ts, kind: e.kind.clone() });
            }
            None => { /* unparseable target: treat as a no-op, path stays put */ }
        },
        // "read" is an access, not a mutation; an unrecognised kind is treated the same way (no-op)
        // rather than panicking, since the fold must stay forward-compatible with new event kinds.
        _ => {}
    }
}

/// Parse a `renamed` event's target path out of `detail`, per the `"-> <target>"` convention
/// documented on the module. `None` if `detail` is absent or doesn't carry the marker.
fn rename_target(e: &AuditEvent) -> Option<String> {
    let detail = e.detail.as_deref()?;
    let target = detail.strip_prefix(RENAME_TARGET_MARKER)?;
    if target.is_empty() {
        None
    } else {
        Some(target.to_string())
    }
}

/// The `(min_ts, max_ts)` span covered by `events`. `None` if `events` is empty.
pub fn bounds(events: &[AuditEvent]) -> Option<(u64, u64)> {
    if events.is_empty() {
        return None;
    }
    let mut min = u64::MAX;
    let mut max = 0u64;
    for e in events {
        min = min.min(e.ts);
        max = max.max(e.ts);
    }
    Some((min, max))
}

/// Count of events with `ts <= t_ms` — the index a scrubber at time `t_ms` should seek to in the
/// ts-sorted stream. Sorts a copy first so unsorted input still yields a meaningful count.
pub fn seek_index(events: &[AuditEvent], t_ms: u64) -> usize {
    let mut ts: Vec<u64> = events.iter().map(|e| e.ts).collect();
    ts.sort_unstable();
    ts.partition_point(|&t| t <= t_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(ts: u64, kind: &str, path: &str) -> AuditEvent {
        AuditEvent { ts, session: "s1".into(), kind: kind.into(), path: path.into(), actor: None, detail: None }
    }

    fn renamed(ts: u64, from: &str, to: &str) -> AuditEvent {
        AuditEvent {
            ts,
            session: "s1".into(),
            kind: "renamed".into(),
            path: from.into(),
            actor: None,
            detail: Some(format!("-> {to}")),
        }
    }

    #[test]
    fn empty_stream_yields_empty_state_no_panic() {
        assert_eq!(state_at(&[], 0), FsState::new());
        assert_eq!(state_at(&[], u64::MAX), FsState::new());
    }

    #[test]
    fn created_appears_at_or_after_its_ts() {
        let events = [ev(10, "created", "/a")];
        assert!(state_at(&events, 9).is_empty());
        assert!(state_at(&events, 10).contains_key("/a"));
        assert!(state_at(&events, 100).contains_key("/a"));
    }

    #[test]
    fn removed_deletes_the_path() {
        let events = [ev(10, "created", "/a"), ev(20, "removed", "/a")];
        assert!(state_at(&events, 15).contains_key("/a"));
        assert!(!state_at(&events, 20).contains_key("/a"));
        assert!(!state_at(&events, 100).contains_key("/a"));
    }

    #[test]
    fn renamed_relocates_old_gone_target_present() {
        let events = [ev(10, "created", "/a"), renamed(20, "/a", "/b")];
        let s15 = state_at(&events, 15);
        assert!(s15.contains_key("/a") && !s15.contains_key("/b"));
        let s20 = state_at(&events, 20);
        assert!(!s20.contains_key("/a"));
        assert!(s20.contains_key("/b"));
        assert_eq!(s20["/b"].kind, "renamed");
        assert_eq!(s20["/b"].ts, 20);
    }

    #[test]
    fn renamed_with_unparseable_detail_is_a_no_op() {
        // Missing "-> " marker entirely.
        let bad1 =
            AuditEvent { ts: 20, session: "s1".into(), kind: "renamed".into(), path: "/a".into(), actor: None, detail: Some("nonsense".into()) };
        // No detail at all.
        let bad2 = ev(20, "renamed", "/a");
        for bad in [bad1, bad2] {
            let events = [ev(10, "created", "/a"), bad];
            let s = state_at(&events, 20);
            // The path stays put — neither deleted nor moved.
            assert!(s.contains_key("/a"));
            assert_eq!(s.len(), 1);
        }
    }

    #[test]
    fn read_is_a_no_op() {
        let events = [ev(10, "created", "/a"), ev(20, "read", "/a")];
        let s = state_at(&events, 20);
        assert_eq!(s.len(), 1);
        // The read must not overwrite the last-touch ts/kind recorded by `created`.
        assert_eq!(s["/a"].ts, 10);
        assert_eq!(s["/a"].kind, "created");
    }

    #[test]
    fn modified_updates_ts_and_kind() {
        let events = [ev(10, "created", "/a"), ev(30, "modified", "/a")];
        let s = state_at(&events, 30);
        assert_eq!(s["/a"].ts, 30);
        assert_eq!(s["/a"].kind, "modified");
    }

    #[test]
    fn modified_inserts_when_absent() {
        // e.g. the `created` event fell outside the journal's retention window.
        let events = [ev(30, "modified", "/a")];
        let s = state_at(&events, 30);
        assert_eq!(s["/a"].ts, 30);
        assert_eq!(s["/a"].kind, "modified");
    }

    #[test]
    fn state_at_correct_across_several_moments() {
        let events = [
            ev(10, "created", "/a"),
            ev(20, "created", "/b"),
            ev(30, "modified", "/a"),
            renamed(40, "/b", "/c"),
            ev(50, "removed", "/a"),
        ];
        assert!(state_at(&events, 5).is_empty());
        let t15: Vec<_> = state_at(&events, 15).into_keys().collect();
        assert_eq!(t15, vec!["/a".to_string()]);
        let t25: Vec<_> = state_at(&events, 25).into_keys().collect();
        assert_eq!(t25, vec!["/a".to_string(), "/b".to_string()]);
        let t45: Vec<_> = state_at(&events, 45).into_keys().collect();
        assert_eq!(t45, vec!["/a".to_string(), "/c".to_string()]);
        let t100: Vec<_> = state_at(&events, 100).into_keys().collect();
        assert_eq!(t100, vec!["/c".to_string()]);
    }

    #[test]
    fn unsorted_input_folds_in_ts_order_same_as_sorted() {
        let sorted = [
            ev(10, "created", "/a"),
            ev(20, "created", "/b"),
            ev(30, "modified", "/a"),
            renamed(40, "/b", "/c"),
        ];
        let mut unsorted = sorted.to_vec();
        unsorted.reverse();
        for t in [0, 10, 15, 20, 25, 30, 35, 40, 100] {
            assert_eq!(state_at(&sorted, t), state_at(&unsorted, t), "mismatch at t={t}");
        }
    }

    #[test]
    fn bounds_of_empty_is_none() {
        assert_eq!(bounds(&[]), None);
    }

    #[test]
    fn bounds_returns_min_and_max_ts() {
        let events = [ev(50, "created", "/a"), ev(10, "created", "/b"), ev(30, "created", "/c")];
        assert_eq!(bounds(&events), Some((10, 50)));
    }

    #[test]
    fn bounds_single_event_is_ts_ts() {
        let events = [ev(42, "created", "/a")];
        assert_eq!(bounds(&events), Some((42, 42)));
    }

    #[test]
    fn seek_index_counts_events_at_or_before_t() {
        let events = [ev(10, "created", "/a"), ev(20, "created", "/b"), ev(30, "created", "/c")];
        assert_eq!(seek_index(&events, 0), 0);
        assert_eq!(seek_index(&events, 9), 0);
        assert_eq!(seek_index(&events, 10), 1);
        assert_eq!(seek_index(&events, 25), 2);
        assert_eq!(seek_index(&events, 30), 3);
        assert_eq!(seek_index(&events, 1_000), 3);
    }

    #[test]
    fn seek_index_of_empty_is_zero() {
        assert_eq!(seek_index(&[], 0), 0);
    }

    #[test]
    fn seek_index_handles_unsorted_input() {
        let events = [ev(30, "created", "/c"), ev(10, "created", "/a"), ev(20, "created", "/b")];
        assert_eq!(seek_index(&events, 20), 2);
    }
}
