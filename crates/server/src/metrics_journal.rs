//! On-disk append-only per-session **cost/activity** metrics journal (CPE-1113, epic CPE-731 slice b).
//!
//! A **sibling** of [`crate::audit_journal`] — same durable-JSONL pattern (temp-file+rename trim,
//! bounded/rotated, corrupt-line-skipping), but a different **grain** and a different file: the audit
//! journal records *many events per session* (one file per session under `audit/`); this journal records
//! *one row per session* (a single append-only `history.jsonl` under `agent-metrics/`). The grain
//! mismatch is exactly why the CPE-731 plan keeps them separate — do NOT fold metrics into the audit file.
//!
//! One [`SessionMetricsRecord`] is appended when a watched Agent Watch session ends (flushed from the
//! frontend's live accumulator, `src/lib/agentSessionMetrics.ts`), so the cost dashboard (CPE-731c) can
//! read cross-session history back after an app restart. The figures are **advisory / best-effort**
//! (tokens+cost are text-scraped from the agent's own output, churn/files/wall-clock are approximations)
//! — never billing. Pure helpers over an explicit base dir with no Tauri deps, so append / read / rotation
//! are exhaustively unit-tested; the Tauri commands in `lib.rs` are a thin `spawn_blocking` shell.
//!
//! **Bounded:** the history file is capped at `max_rows` lines; appending past the cap rotates the oldest
//! rows out in place, so an always-on Agent Watch can't grow the journal without limit. The journal is
//! only written when a session ends — it costs nothing when Agent Watch is off ("off means off").

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// One session's final cost + activity tallies. Mirrors the frontend `SessionMetrics`
/// (`src/lib/agentSessionMetrics.ts`) — camelCase on the wire to match it. **Advisory / best-effort:**
/// tokens + cost are text-scraped from the agent's own output; churn / files / wall-clock are
/// approximations. Not billing.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SessionMetricsRecord {
    /// The agent session id this row belongs to.
    pub session_id: String,
    /// Agent catalog id (e.g. `"claude"`).
    pub agent_id: String,
    /// Human-readable agent name (e.g. `"Claude Code"`).
    pub agent_name: String,
    /// Provider the session ran through (e.g. `"openrouter"`).
    pub provider: String,
    /// Model the session used (e.g. `"sonnet"`).
    pub model: String,
    /// The session's working directory.
    pub cwd: String,
    /// Epoch ms the session started (folded from its `started` announcement).
    pub started_at: u64,
    /// Epoch ms the session ended (folded from its `ended` announcement / teardown).
    pub ended_at: u64,
    /// `ended_at - started_at` wall-clock, in milliseconds (best-effort — misses think time).
    pub wall_clock_ms: u64,
    /// Best-effort input tokens (text-scraped, advisory).
    pub input_tokens: u64,
    /// Best-effort output tokens (text-scraped, advisory).
    pub output_tokens: u64,
    /// `input_tokens + output_tokens`.
    pub total_tokens: u64,
    /// Best-effort USD cost (text-scraped, advisory — never billing).
    pub cost_usd: f64,
    /// Distinct paths the session wrote to.
    pub files_touched: u64,
    /// Σ |after.len − before.len| across every write (approximate churn).
    pub churn_bytes: u64,
    /// Total fs-diff writes attributed to the session (not deduped by path).
    pub edit_count: u64,
}

/// Default cap on session rows retained before the oldest are rotated out. A generous history — a row is
/// tiny (~one JSON line) and only written once per session end.
pub const MAX_SESSIONS: usize = 5_000;

/// The single append-only history file inside `base`.
fn history_file(base: &Path) -> PathBuf {
    base.join("history.jsonl")
}

/// Append one session's metrics row to the history journal (creating `base` if needed), then trim the
/// file to its last `max_rows` lines. Durable: the line is flushed before returning. `max_rows == 0`
/// disables trimming.
pub fn append(base: &Path, rec: &SessionMetricsRecord, max_rows: usize) -> Result<(), String> {
    fs::create_dir_all(base).map_err(|e| e.to_string())?;
    let file = history_file(base);
    let line = serde_json::to_string(rec).map_err(|e| e.to_string())?;
    {
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .map_err(|e| e.to_string())?;
        writeln!(f, "{line}").map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
    }
    trim(&file, max_rows)?;
    Ok(())
}

/// Keep only the last `max_rows` non-empty lines of the history file (rotate the oldest out). Rewrites
/// via a temp file + rename so a crash can't leave a half-truncated journal.
fn trim(file: &Path, max_rows: usize) -> Result<(), String> {
    if max_rows == 0 {
        return Ok(());
    }
    let content = fs::read_to_string(file).map_err(|e| e.to_string())?;
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() <= max_rows {
        return Ok(());
    }
    let keep = &lines[lines.len() - max_rows..];
    let tmp = file.with_extension("jsonl.tmp");
    let mut body = keep.join("\n");
    body.push('\n');
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    fs::rename(&tmp, file).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read every session row back, in append order (oldest → newest), skipping malformed lines (robust to a
/// partial trailing write). Missing dir/file → empty.
pub fn read_all(base: &Path) -> Vec<SessionMetricsRecord> {
    let file = history_file(base);
    let content = match fs::read_to_string(&file) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<SessionMetricsRecord>(l).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        // A process-wide atomic counter guarantees a unique dir per call (matches audit_journal's tmp()):
        // cargo runs tests in parallel and a coarse clock can mint duplicate time-based names.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-metrics-{}-{}", std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn rec(session: &str, started: u64, ended: u64) -> SessionMetricsRecord {
        SessionMetricsRecord {
            session_id: session.into(),
            agent_id: "claude".into(),
            agent_name: "Claude Code".into(),
            provider: "openrouter".into(),
            model: "sonnet".into(),
            cwd: "/work".into(),
            started_at: started,
            ended_at: ended,
            wall_clock_ms: ended - started,
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            cost_usd: 1.25,
            files_touched: 3,
            churn_bytes: 2048,
            edit_count: 7,
        }
    }

    #[test]
    fn append_then_read_all_preserves_order_and_round_trips() {
        let base = tmp();
        for i in 1..=3 {
            append(&base, &rec(&format!("s{i}"), i * 1000, i * 1000 + 500), MAX_SESSIONS).unwrap();
        }
        let got = read_all(&base);
        assert_eq!(got.iter().map(|r| r.session_id.clone()).collect::<Vec<_>>(), vec!["s1", "s2", "s3"]);
        // Full field round-trip on the first row.
        assert_eq!(got[0], rec("s1", 1000, 1500));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn journal_survives_a_fresh_read_handle() {
        // "Restart": nothing held in memory; a brand-new read sees the persisted rows.
        let base = tmp();
        append(&base, &rec("boot", 10, 20), MAX_SESSIONS).unwrap();
        assert_eq!(read_all(&base).len(), 1);
        assert_eq!(read_all(&base)[0].session_id, "boot");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn cap_rotation_keeps_the_newest() {
        let base = tmp();
        for i in 1..=5u64 {
            append(&base, &rec(&format!("s{i}"), i * 1000, i * 1000 + 100), 3).unwrap(); // cap 3
        }
        let got = read_all(&base);
        assert_eq!(
            got.iter().map(|r| r.session_id.clone()).collect::<Vec<_>>(),
            vec!["s3", "s4", "s5"]
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn read_skips_malformed_lines() {
        let base = tmp();
        append(&base, &rec("s1", 1000, 1100), 100).unwrap();
        // Splice a garbage/partial line between two valid rows (a torn write).
        let file = history_file(&base);
        let mut f = OpenOptions::new().append(true).open(&file).unwrap();
        writeln!(f, "{{ this is not json").unwrap();
        drop(f);
        append(&base, &rec("s2", 2000, 2200), 100).unwrap();
        let got = read_all(&base);
        assert_eq!(got.iter().map(|r| r.session_id.clone()).collect::<Vec<_>>(), vec!["s1", "s2"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn missing_dir_reads_empty() {
        let base = std::env::temp_dir().join(format!("cpe-metrics-missing-{}", std::process::id()));
        fs::remove_dir_all(&base).ok();
        assert!(read_all(&base).is_empty());
        // Reading must not create the dir.
        assert!(!base.exists());
    }

    #[test]
    fn camelcase_wire_shape_matches_the_frontend() {
        // The record serializes camelCase (sessionId/wallClockMs/…) to line up with `SessionMetrics`.
        let line = serde_json::to_string(&rec("s1", 1000, 4000)).unwrap();
        for key in ["sessionId", "agentId", "agentName", "startedAt", "endedAt", "wallClockMs",
            "inputTokens", "outputTokens", "totalTokens", "costUsd", "filesTouched", "churnBytes",
            "editCount"] {
            assert!(line.contains(key), "expected camelCase key `{key}` in {line}");
        }
        assert!(!line.contains("session_id"), "must not emit snake_case keys: {line}");
    }
}
