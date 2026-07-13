//! Session persistence & history (CPE-292).
//!
//! Remembers console sessions and their transcripts across restarts, per repo, in the
//! sidecar's own storage namespace (CPE-269). Growth is bounded (a session cap + a
//! per-transcript byte cap) so history can't grow without limit, and **secrets are
//! redacted from transcripts** before they are ever stored — the injected API keys are
//! scrubbed (a sidecar-local redactor, since sidecars depend only on the contract).

use serde::{Deserialize, Serialize};

/// History schema version (CPE-300 discipline).
pub const HISTORY_SCHEMA_VERSION: u16 = 1;

/// How many sessions to keep before rotating out the oldest.
pub const SESSION_CAP: usize = 50;

/// Max stored transcript length; older output is trimmed from the front.
pub const TRANSCRIPT_CAP: usize = 64 * 1024;

/// Replacement for redacted secret values.
pub const REDACTED: &str = "***";

/// Scrub each known secret value out of `text`. Empty secrets are ignored. Used on
/// every transcript before it is stored, so an echoed key never lands on disk.
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for s in secrets {
        if !s.is_empty() && out.contains(s.as_str()) {
            out = out.replace(s.as_str(), REDACTED);
        }
    }
    out
}

/// One recorded console session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub agent: String,
    pub provider: String,
    pub model: Option<String>,
    pub cwd: String,
    /// ISO-ish timestamp string (the host supplies it; we don't parse it).
    pub started_at: String,
    /// Redacted, length-capped transcript.
    pub transcript: String,
}

/// Per-repo session history, persisted as JSON via the storage capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistory {
    pub schema_version: u16,
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
}

impl Default for SessionHistory {
    fn default() -> Self {
        Self { schema_version: HISTORY_SCHEMA_VERSION, sessions: Vec::new() }
    }
}

impl SessionHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a session, redacting its transcript against `secrets`, trimming it to the
    /// byte cap, and rotating out the oldest session past [`SESSION_CAP`].
    pub fn record(&mut self, mut record: SessionRecord, secrets: &[String]) {
        record.transcript = redact(&record.transcript, secrets);
        record.transcript = trim_front(&record.transcript, TRANSCRIPT_CAP);
        self.sessions.push(record);
        while self.sessions.len() > SESSION_CAP {
            self.sessions.remove(0);
        }
    }

    /// Sessions most-recent first.
    pub fn recent(&self) -> impl Iterator<Item = &SessionRecord> {
        self.sessions.iter().rev()
    }

    pub fn clear(&mut self) {
        self.sessions.clear();
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

/// Keep only the last `cap` bytes of `s`, on a char boundary.
fn trim_front(s: &str, cap: usize) -> String {
    if s.len() <= cap {
        return s.to_string();
    }
    let mut start = s.len() - cap;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, transcript: &str) -> SessionRecord {
        SessionRecord {
            id: id.into(),
            agent: "claude".into(),
            provider: "openrouter".into(),
            model: Some("m".into()),
            cwd: "/repo".into(),
            started_at: "2026-07-13T10:00:00Z".into(),
            transcript: transcript.into(),
        }
    }

    #[test]
    fn redacts_secrets_from_a_stored_transcript() {
        let mut h = SessionHistory::new();
        h.record(rec("1", "auth with sk-or-supersecret ok"), &["sk-or-supersecret".into()]);
        assert!(h.sessions[0].transcript.contains("***"));
        assert!(!h.sessions[0].transcript.contains("supersecret"));
    }

    #[test]
    fn rotates_out_the_oldest_past_the_cap() {
        let mut h = SessionHistory::new();
        for i in 0..SESSION_CAP + 5 {
            h.record(rec(&i.to_string(), "x"), &[]);
        }
        assert_eq!(h.len(), SESSION_CAP);
        // The oldest (ids 0..5) rotated out; newest is last.
        assert_eq!(h.sessions.first().unwrap().id, "5");
        assert_eq!(h.recent().next().unwrap().id, (SESSION_CAP + 4).to_string());
    }

    #[test]
    fn caps_a_huge_transcript() {
        let mut h = SessionHistory::new();
        let big = "a".repeat(TRANSCRIPT_CAP * 2);
        h.record(rec("1", &big), &[]);
        assert_eq!(h.sessions[0].transcript.len(), TRANSCRIPT_CAP);
    }

    #[test]
    fn round_trips_through_json_and_clears() {
        let mut h = SessionHistory::new();
        h.record(rec("1", "hi"), &[]);
        let json = serde_json::to_string(&h).unwrap();
        let back: SessionHistory = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_version, HISTORY_SCHEMA_VERSION);
        assert_eq!(back.len(), 1);
        h.clear();
        assert!(h.is_empty());
    }
}
