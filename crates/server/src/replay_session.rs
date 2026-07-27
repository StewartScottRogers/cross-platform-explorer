//! Journal-backed replay assembly (CPE-1066, epic CPE-728): read a session's durable audit journal
//! back from disk and package it for replay — events, time bounds, and a summary. This is the
//! assembly seam a future `#[tauri::command]` dispatches into; it does no I/O of its own beyond what
//! `audit_journal::read_session` already performs, and adds no new deps.
//!
//! Reuses three already-tested pure modules:
//! - [`crate::audit_journal::read_session`] — read the events back from the on-disk journal.
//! - [`crate::replay::bounds`] — the `(min_ts, max_ts)` span the events cover.
//! - [`crate::activity_timeline::summarize`] — totals / by-kind / sessions / span.
//!
//! A missing or empty session journal yields an empty [`ReplayData`] (empty events, `bounds: None`,
//! an empty summary) rather than an error — there's nothing exceptional about a session that hasn't
//! recorded anything yet, or a replay request that races a restart before the journal dir exists.

use crate::activity_timeline::{self, ActivitySummary};
use crate::audit_journal::{self, AuditEvent};
use crate::replay;
use std::path::Path;

/// Everything a replay view needs for one session, assembled from its durable audit journal.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReplayData {
    /// The session's recorded events, in journal (append) order.
    pub events: Vec<AuditEvent>,
    /// The `(min_ts, max_ts)` span covered by `events`. `None` if `events` is empty.
    pub bounds: Option<(u64, u64)>,
    /// Whole-run summary (totals / by-kind / sessions / span) over `events`.
    pub summary: ActivitySummary,
}

/// Read `session`'s durable audit journal back from disk under `base` and package it for replay:
/// the events in journal order, their time bounds, and a summary. A missing or empty session
/// journal yields an empty [`ReplayData`] — never a panic or an error.
pub fn load_replay(base: &Path, session: &str) -> ReplayData {
    let events = audit_journal::read_session(base, session);
    let bounds = replay::bounds(&events);
    let summary = activity_timeline::summarize(&events);
    ReplayData { events, bounds, summary }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn tmp() -> PathBuf {
        // Mirror `audit_journal`'s own test helper: a process-wide atomic counter guarantees a unique
        // dir per call, since cargo runs tests in parallel and a coarse clock (macOS) can otherwise
        // collide two time-based names sharing the same `process::id()`.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-replay-session-{}-{}", std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn ev(ts: u64, session: &str, kind: &str) -> AuditEvent {
        AuditEvent { ts, session: session.into(), kind: kind.into(), path: format!("/x/{ts}.txt"), actor: None, detail: None }
    }

    #[test]
    fn loads_events_in_order_with_bounds_and_summary() {
        let base = tmp();
        // Write via the journal's own append path (not a hand-rolled file), mirroring how a real
        // session's journal would be built.
        audit_journal::record(&base, &ev(10, "s1", "created"), audit_journal::MAX_EVENTS_PER_SESSION).unwrap();
        audit_journal::record(&base, &ev(20, "s1", "modified"), audit_journal::MAX_EVENTS_PER_SESSION).unwrap();
        audit_journal::record(&base, &ev(30, "s1", "removed"), audit_journal::MAX_EVENTS_PER_SESSION).unwrap();

        let data = load_replay(&base, "s1");

        assert_eq!(data.events.iter().map(|e| e.ts).collect::<Vec<_>>(), vec![10, 20, 30]);
        assert_eq!(data.bounds, Some((10, 30)));
        assert_eq!(data.summary.total, 3);
        assert_eq!(data.summary.by_kind["created"], 1);
        assert_eq!(data.summary.by_kind["modified"], 1);
        assert_eq!(data.summary.by_kind["removed"], 1);
        assert_eq!(data.summary.sessions, ["s1".to_string()].into_iter().collect());
        assert_eq!(data.summary.span_ms, 20);

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn missing_session_yields_empty_data_no_panic() {
        let base = tmp();
        // Nothing ever recorded under this base at all — the dir itself may not even exist.
        let data = load_replay(&base, "nope");
        assert!(data.events.is_empty());
        assert_eq!(data.bounds, None);
        assert_eq!(data.summary, ActivitySummary::default());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn empty_session_among_others_yields_empty_data() {
        let base = tmp();
        // Another session's journal exists under the same base, but the one we ask for doesn't.
        audit_journal::record(&base, &ev(1, "other", "created"), audit_journal::MAX_EVENTS_PER_SESSION).unwrap();
        let data = load_replay(&base, "s1");
        assert!(data.events.is_empty());
        assert_eq!(data.bounds, None);
        assert_eq!(data.summary, ActivitySummary::default());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn single_event_session_has_zero_span_and_equal_bounds() {
        let base = tmp();
        audit_journal::record(&base, &ev(42, "s1", "created"), audit_journal::MAX_EVENTS_PER_SESSION).unwrap();
        let data = load_replay(&base, "s1");
        assert_eq!(data.bounds, Some((42, 42)));
        assert_eq!(data.summary.span_ms, 0);
        assert_eq!(data.summary.total, 1);
        fs::remove_dir_all(&base).ok();
    }

    /// QA-Architect end-to-end pin for the whole CPE-728 replay chain (journal write → baseline → load →
    /// fold → folder listing). Every stage is unit-tested in isolation; this ties them together so a wiring
    /// bug BETWEEN the journal, the baseline seed, and the reconstruction fold can't slip through unnoticed.
    #[test]
    fn end_to_end_journal_baseline_load_reconstruct_folder_at_each_moment() {
        let base = tmp();
        let session = "e2e";

        // Baseline: two pre-existing files under the watched root /w, exactly as `capture` would record them.
        let baseline = crate::replay_baseline::Baseline {
            paths: vec!["/w/a.txt".into(), "/w/b.txt".into()],
            truncated: false,
        };
        crate::replay_baseline::write_baseline(&base, session, &baseline).unwrap();

        // Events via the journal's real append path (record_many): create c@10, remove b@20, rename c→d@30.
        let events = vec![
            AuditEvent { ts: 10, session: session.into(), kind: "created".into(), path: "/w/c.txt".into(), actor: None, detail: None },
            AuditEvent { ts: 20, session: session.into(), kind: "removed".into(), path: "/w/b.txt".into(), actor: None, detail: None },
            AuditEvent { ts: 30, session: session.into(), kind: "renamed".into(), path: "/w/c.txt".into(), actor: None, detail: Some("-> /w/d.txt".into()) },
        ];
        audit_journal::record_many(&base, &events, audit_journal::MAX_EVENTS_PER_SESSION).unwrap();

        // Load back exactly as the Replay tab would (read_baseline + load_replay).
        let loaded_baseline = crate::replay_baseline::read_baseline(&base, session).expect("baseline round-trips");
        assert_eq!(loaded_baseline.paths.len(), 2);
        let data = load_replay(&base, session);
        assert_eq!(data.events.len(), 3);

        // Reconstruct the /w listing at each scrub moment (basenames; children_at sorts ascending by path).
        let names_at = |t: u64| -> Vec<String> {
            let state = replay::state_at_from(&loaded_baseline.to_state(), &data.events, t);
            crate::replay_view::children_at(&state, "/w").into_iter().map(|e| e.name).collect()
        };
        assert_eq!(names_at(5), vec!["a.txt", "b.txt"], "before any event → baseline only");
        assert_eq!(names_at(15), vec!["a.txt", "b.txt", "c.txt"], "after create c");
        assert_eq!(names_at(25), vec!["a.txt", "c.txt"], "after remove b");
        assert_eq!(names_at(35), vec!["a.txt", "d.txt"], "after rename c→d");

        fs::remove_dir_all(&base).ok();
    }
}
