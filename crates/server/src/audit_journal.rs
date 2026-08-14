//! On-disk append-only session audit journal (CPE-800, epic CPE-733).
//!
//! Durably records each session's filesystem-activity events as JSON-lines — one file per session — so
//! the history browser + export (CPE-799 / CPE-801) can read past sessions back after an app restart.
//! Pure helpers over an explicit base dir with no Tauri deps, so append / list / read / rotation are
//! exhaustively unit-tested; the Tauri commands in `lib.rs` are a thin shell that resolve the journal
//! dir (under the app-data dir) and delegate here. Shares the event model with the frontend
//! (`src/lib/auditExport.ts`) and with replay (CPE-728).
//!
//! **Bounded:** each session file is capped at `max_events` lines; appending past the cap rotates the
//! oldest events out in place, so an always-on Agent Watch can't grow the journal without limit. The
//! journal is only written when Agent Watch records activity — it costs nothing when the mode is off.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// One filesystem-activity event. Mirrors the frontend `AuditEvent` (`src/lib/auditExport.ts`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AuditEvent {
    /// Epoch milliseconds.
    pub ts: u64,
    /// Session id.
    pub session: String,
    /// Activity kind (created / modified / removed / renamed / read).
    pub kind: String,
    /// Absolute path the activity touched.
    pub path: String,
    /// Who caused the event (CPE-1101 actor attribution: the owning session id, `"user"`, or
    /// `"unknown"`). Additive + backward-compatible: old journal lines with no `actor` deserialize
    /// as `None`, and the replay fold ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Optional extra detail (rename target, diff summary, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Default cap on events retained per session before the oldest are rotated out.
pub const MAX_EVENTS_PER_SESSION: usize = 10_000;

/// Map a session id to a safe single-segment journal file inside `base`. Non-alphanumeric characters
/// (other than `-` / `_`) are replaced so a session id can never escape the journal dir.
fn session_file(base: &Path, session: &str) -> PathBuf {
    let safe: String = session
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    base.join(format!("{safe}.jsonl"))
}

/// Append one event to its session journal (creating `base` if needed), then trim the file to its last
/// `max_events` lines. Durable: the line is flushed before returning. `max_events == 0` disables trimming.
pub fn record(base: &Path, event: &AuditEvent, max_events: usize) -> Result<(), String> {
    fs::create_dir_all(base).map_err(|e| e.to_string())?;
    let file = session_file(base, &event.session);
    let line = serde_json::to_string(event).map_err(|e| e.to_string())?;
    {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .map_err(|e| e.to_string())?;
        writeln!(f, "{line}").map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
    }
    trim(&file, max_events)?;
    Ok(())
}

/// Append a whole batch of events to their session journals in one call (creating `base` if needed),
/// then trim each touched file to its last `max_events` lines once. A 50-file watch batch becomes one
/// call instead of 50, keeping the same per-session cap/rotation as [`record`]. Events are grouped by
/// session so mixed-session batches stay correct; empty batches are a no-op (they never create `base`).
pub fn record_many(base: &Path, events: &[AuditEvent], max_events: usize) -> Result<(), String> {
    if events.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(base).map_err(|e| e.to_string())?;
    // Append every event first, remembering which session files were touched, then trim each once.
    let mut touched: Vec<String> = Vec::new();
    for event in events {
        let file = session_file(base, &event.session);
        let line = serde_json::to_string(event).map_err(|e| e.to_string())?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .map_err(|e| e.to_string())?;
        writeln!(f, "{line}").map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
        if !touched.iter().any(|s| s == &event.session) {
            touched.push(event.session.clone());
        }
    }
    for session in &touched {
        trim(&session_file(base, session), max_events)?;
    }
    Ok(())
}

/// Keep only the last `max_events` non-empty lines of a journal file (rotate the oldest out). Rewrites
/// via a temp file + rename so a crash can't leave a half-truncated journal.
fn trim(file: &Path, max_events: usize) -> Result<(), String> {
    if max_events == 0 {
        return Ok(());
    }
    let content = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() <= max_events {
        return Ok(());
    }
    let keep = &lines[lines.len() - max_events..];
    let tmp = file.with_extension("jsonl.tmp");
    let mut body = keep.join("\n");
    body.push('\n');
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    // CPE-1710: app-private journal. `file` is `app_data_dir()/audit/<sanitised session id>` — the
    // caller cannot steer it onto a user-named path, and the tmp -> final replace is the atomicity this
    // write depends on.
    #[allow(clippy::disallowed_methods)]
    fs::rename(&tmp, file).map_err(|e| e.to_string())?;
    Ok(())
}

/// List the session ids that have a journal file under `base`, sorted. Missing dir → empty.
pub fn list_sessions(base: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(base) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// Read every event for one session back, in append order, skipping malformed lines (robust to a partial
/// trailing write). Missing session → empty.
pub fn read_session(base: &Path, session: &str) -> Vec<AuditEvent> {
    let file = session_file(base, session);
    let content = match fs::read_to_string(&file) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<AuditEvent>(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        // A process-wide atomic counter guarantees a unique dir per call. Time-based names are NOT safe
        // here: cargo runs tests in parallel and `process::id()` is shared, so on a platform with a coarse
        // clock (macOS) two tests can mint the same nanos and clobber each other's `s1.jsonl`.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-audit-{}-{}", std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn ev(ts: u64, session: &str, kind: &str) -> AuditEvent {
        AuditEvent {
            ts,
            session: session.into(),
            kind: kind.into(),
            path: format!("/x/{ts}.txt"),
            actor: None,
            detail: None,
        }
    }

    #[test]
    fn append_then_read_preserves_order() {
        let base = tmp();
        for ts in 1..=3 {
            record(&base, &ev(ts, "s1", "modified"), MAX_EVENTS_PER_SESSION).unwrap();
        }
        let got = read_session(&base, "s1");
        assert_eq!(got.iter().map(|e| e.ts).collect::<Vec<_>>(), vec![1, 2, 3]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn journal_survives_a_fresh_read_handle() {
        // Simulates "restart": nothing is held in memory; a brand-new read sees the persisted events.
        let base = tmp();
        record(&base, &ev(7, "boot", "created"), MAX_EVENTS_PER_SESSION).unwrap();
        assert_eq!(read_session(&base, "boot").len(), 1);
        assert_eq!(read_session(&base, "boot")[0].ts, 7);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn bounded_rotation_keeps_the_newest() {
        let base = tmp();
        for ts in 1..=5 {
            record(&base, &ev(ts, "s1", "read"), 3).unwrap(); // cap 3
        }
        let got = read_session(&base, "s1");
        assert_eq!(got.iter().map(|e| e.ts).collect::<Vec<_>>(), vec![3, 4, 5]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn lists_sessions_sorted() {
        let base = tmp();
        record(&base, &ev(1, "zebra", "read"), 100).unwrap();
        record(&base, &ev(1, "alpha", "read"), 100).unwrap();
        assert_eq!(list_sessions(&base), vec!["alpha".to_string(), "zebra".to_string()]);
        assert!(list_sessions(&base.join("missing")).is_empty());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn read_skips_malformed_lines() {
        let base = tmp();
        // Two valid events with a truncated/garbage line spliced between them (a partial write).
        record(&base, &ev(1, "s1", "created"), 100).unwrap();
        let file = session_file(&base, "s1");
        let mut f = OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(f, "{{ this is not json").unwrap();
        drop(f);
        record(&base, &ev(2, "s1", "created"), 100).unwrap();
        let got = read_session(&base, "s1");
        assert_eq!(got.iter().map(|e| e.ts).collect::<Vec<_>>(), vec![1, 2]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn detail_round_trips_and_missing_session_is_empty() {
        let base = tmp();
        let mut e = ev(9, "s1", "renamed");
        e.detail = Some("-> /x/new.txt".into());
        record(&base, &e, 100).unwrap();
        assert_eq!(read_session(&base, "s1")[0].detail.as_deref(), Some("-> /x/new.txt"));
        assert!(read_session(&base, "nope").is_empty());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn record_many_appends_the_whole_batch_in_order() {
        let base = tmp();
        let batch: Vec<AuditEvent> = (1..=5).map(|ts| ev(ts, "s1", "modified")).collect();
        record_many(&base, &batch, MAX_EVENTS_PER_SESSION).unwrap();
        let got = read_session(&base, "s1");
        assert_eq!(got.iter().map(|e| e.ts).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn record_many_respects_the_cap_keeping_the_newest() {
        let base = tmp();
        // Two batches totalling 6 events into a cap-4 journal: only the last 4 survive.
        record_many(&base, &(1..=3).map(|ts| ev(ts, "s1", "read")).collect::<Vec<_>>(), 4).unwrap();
        record_many(&base, &(4..=6).map(|ts| ev(ts, "s1", "read")).collect::<Vec<_>>(), 4).unwrap();
        let got = read_session(&base, "s1");
        assert_eq!(got.iter().map(|e| e.ts).collect::<Vec<_>>(), vec![3, 4, 5, 6]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn record_many_groups_mixed_sessions_and_caps_each() {
        let base = tmp();
        // Interleaved sessions in one batch, cap 2: each session file keeps only its own last 2.
        let batch = vec![
            ev(1, "a", "created"),
            ev(2, "b", "created"),
            ev(3, "a", "created"),
            ev(4, "b", "created"),
            ev(5, "a", "created"),
        ];
        record_many(&base, &batch, 2).unwrap();
        assert_eq!(
            read_session(&base, "a").iter().map(|e| e.ts).collect::<Vec<_>>(),
            vec![3, 5]
        );
        assert_eq!(
            read_session(&base, "b").iter().map(|e| e.ts).collect::<Vec<_>>(),
            vec![2, 4]
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn record_many_empty_batch_is_a_noop_and_creates_nothing() {
        let base = std::env::temp_dir().join(format!("cpe-audit-empty-{}", std::process::id()));
        // Deliberately do NOT create `base`: an empty batch must not touch the filesystem.
        fs::remove_dir_all(&base).ok();
        record_many(&base, &[], MAX_EVENTS_PER_SESSION).unwrap();
        assert!(!base.exists(), "empty record_many must not create the journal dir");
    }

    #[test]
    fn actor_round_trips_and_old_lines_without_actor_read_as_none() {
        let base = tmp();
        // New event carrying an actor round-trips.
        let mut e = ev(1, "s1", "modified");
        e.actor = Some("user".into());
        record(&base, &e, 100).unwrap();
        assert_eq!(read_session(&base, "s1")[0].actor.as_deref(), Some("user"));

        // An "old" journal line predating the field (no `actor` key) still deserializes → None.
        let file = session_file(&base, "s2");
        fs::create_dir_all(&base).unwrap();
        let mut f = OpenOptions::new().create(true).append(true).open(&file).unwrap();
        writeln!(f, r#"{{"ts":2,"session":"s2","kind":"created","path":"/x/2.txt"}}"#).unwrap();
        drop(f);
        let got = read_session(&base, "s2");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].actor, None);
        assert_eq!(got[0].detail, None);
        assert_eq!(got[0].path, "/x/2.txt");

        // And an event with actor None serializes WITHOUT the key (skip_serializing_if).
        let line = serde_json::to_string(&ev(3, "s3", "removed")).unwrap();
        assert!(!line.contains("actor"), "actor:None must be omitted from JSON: {line}");
        fs::remove_dir_all(&base).ok();
    }
}
