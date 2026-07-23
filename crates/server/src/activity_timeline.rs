//! Activity timeline bucketing (CPE-916, epic CPE-728): fold a stream of [`AuditEvent`]s into fixed
//! time-window buckets for the scrubbable "activity replay" view — the compute core the timeline/minimap UI
//! renders and the scrubber steps through. Pure, over already-recorded events.

use crate::audit_journal::AuditEvent;
use std::collections::{BTreeMap, BTreeSet};

/// One time-window of activity.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TimelineBucket {
    /// Bucket start (epoch ms), aligned down to a multiple of the window.
    pub start_ms: u64,
    pub count: usize,
    /// Event count per kind (created / modified / removed / …), sorted for a stable render.
    pub by_kind: BTreeMap<String, usize>,
    /// Distinct sessions active in this window.
    pub sessions: BTreeSet<String>,
}

/// Group `events` into `window_ms`-wide buckets aligned to the epoch, sorted by time. Only non-empty
/// buckets are returned (a gap in activity is a gap in the timeline). A `window_ms` of 0 is treated as 1
/// (each distinct ms is its own bucket) to avoid a divide-by-zero.
pub fn bucketize(events: &[AuditEvent], window_ms: u64) -> Vec<TimelineBucket> {
    let window = window_ms.max(1);
    let mut buckets: BTreeMap<u64, TimelineBucket> = BTreeMap::new();
    for e in events {
        let start = (e.ts / window) * window;
        let b = buckets.entry(start).or_insert_with(|| TimelineBucket { start_ms: start, ..Default::default() });
        b.count += 1;
        *b.by_kind.entry(e.kind.clone()).or_insert(0) += 1;
        b.sessions.insert(e.session.clone());
    }
    buckets.into_values().collect()
}

/// A whole-run summary of an activity stream.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ActivitySummary {
    pub total: usize,
    pub by_kind: BTreeMap<String, usize>,
    pub sessions: BTreeSet<String>,
    /// First → last event span in ms (`0` for 0/1 events).
    pub span_ms: u64,
}

/// Summarise an activity stream: totals, per-kind counts, distinct sessions, and the time span.
pub fn summarize(events: &[AuditEvent]) -> ActivitySummary {
    let mut s = ActivitySummary { total: events.len(), ..Default::default() };
    let (mut min, mut max) = (u64::MAX, 0u64);
    for e in events {
        *s.by_kind.entry(e.kind.clone()).or_insert(0) += 1;
        s.sessions.insert(e.session.clone());
        min = min.min(e.ts);
        max = max.max(e.ts);
    }
    s.span_ms = if events.is_empty() { 0 } else { max - min };
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(ts: u64, session: &str, kind: &str) -> AuditEvent {
        AuditEvent { ts, session: session.into(), kind: kind.into(), path: "/x".into(), detail: None }
    }

    #[test]
    fn buckets_align_to_the_window_and_count_by_kind() {
        let events = [
            ev(1_000, "s1", "created"),
            ev(1_500, "s1", "modified"),
            ev(2_100, "s2", "modified"),
            ev(9_000, "s1", "removed"),
        ];
        // 1000ms windows: [1000] has 2 (created+modified, s1), [2000] has 1 (modified, s2), [9000] has 1.
        let b = bucketize(&events, 1_000);
        assert_eq!(b.len(), 3);
        assert_eq!((b[0].start_ms, b[0].count), (1_000, 2));
        assert_eq!(b[0].by_kind["created"], 1);
        assert_eq!(b[0].by_kind["modified"], 1);
        assert_eq!(b[0].sessions, ["s1".to_string()].into_iter().collect());
        assert_eq!((b[1].start_ms, b[1].count), (2_000, 1));
        assert_eq!((b[2].start_ms, b[2].count), (9_000, 1));
    }

    #[test]
    fn empty_stream_has_no_buckets() {
        assert!(bucketize(&[], 1_000).is_empty());
    }

    #[test]
    fn summarize_totals_kinds_sessions_and_span() {
        let events = [ev(100, "a", "created"), ev(400, "b", "created"), ev(1_100, "a", "removed")];
        let s = summarize(&events);
        assert_eq!(s.total, 3);
        assert_eq!(s.by_kind["created"], 2);
        assert_eq!(s.by_kind["removed"], 1);
        assert_eq!(s.sessions.len(), 2);
        assert_eq!(s.span_ms, 1_000); // 1100 - 100
        assert_eq!(summarize(&[]).span_ms, 0);
    }
}
