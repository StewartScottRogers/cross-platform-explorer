//! Observability: structured logging, correlation, redaction & diagnostics (CPE-298).
//!
//! A huge, always-growing system is only maintainable if failures are diagnosable —
//! and diagnosable **without leaking secrets**. This module provides:
//!
//! - [`Redactor`] — the single, shared secret-scrubbing utility every log/transcript/
//!   diagnostics path must run values through (referenced by the secrets broker
//!   CPE-268 and session history CPE-292).
//! - [`LogRecord`] / [`LogCapture`] — structured, correlation-tagged log records with a
//!   bounded per-sidecar ring buffer.
//! - [`Diagnostics`] / [`build_diagnostics`] — a redacted, shareable "export
//!   diagnostics" bundle (versions + recent logs).

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sidecar_contract::ContractVersion;

/// JSON object keys whose string values are always redacted, regardless of content —
/// defence in depth against a secret slipping through under a well-known name.
const SENSITIVE_KEYS: &[&str] = &[
    "value",
    "secret",
    "password",
    "passwd",
    "token",
    "auth_token",
    "authorization",
    "api_key",
    "apikey",
    "access_key",
    "private_key",
];

/// The placeholder that replaces any redacted content.
pub const REDACTED: &str = "***";

/// Scrubs secrets from strings and JSON. Register the exact secret values you know
/// (e.g. an API key just fetched from the vault) and it removes them anywhere they
/// appear; it also blanks values under [`SENSITIVE_KEYS`] in JSON.
#[derive(Debug, Default, Clone)]
pub struct Redactor {
    secrets: Vec<String>,
}

impl Redactor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a known secret value to scrub. Empty values are ignored (scrubbing
    /// "" would replace everything).
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        let s = secret.into();
        if !s.is_empty() {
            self.secrets.push(s);
        }
        self
    }

    /// Replace every occurrence of every registered secret in `input` with [`REDACTED`].
    pub fn redact_str(&self, input: &str) -> String {
        // Longest secrets first: a shorter secret that is a substring of a longer one would otherwise
        // rewrite part of the longer secret before it can be matched whole, leaking the remaining
        // fragment (e.g. redacting "SECRET" out of "SECRETLONGER" would leave "LONGER").
        let mut ordered: Vec<&String> = self.secrets.iter().collect();
        ordered.sort_by_key(|s| std::cmp::Reverse(s.len()));
        let mut out = input.to_string();
        for secret in ordered {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), REDACTED);
            }
        }
        out
    }

    /// Redact a free-text log line for safe display in a diagnostics panel or bundle.
    ///
    /// First scrubs every *registered* secret ([`redact_str`](Self::redact_str)), then
    /// applies the heuristic [`redact_secret_patterns`] so an **unregistered** secret
    /// can't slip through: bearer tokens, `sensitive_key=value` / `sensitive_key: value`
    /// assignments, and tokens carrying a well-known credential prefix are all masked.
    /// This is the method the `sidecar_diagnostics` command runs every captured line
    /// through — here over-redaction is the safe failure (a diagnostics view that hides
    /// one word beats one that leaks a key).
    pub fn redact_log_line(&self, input: &str) -> String {
        redact_secret_patterns(&self.redact_str(input))
    }

    /// Redact a JSON value in place: blank the string values of [`SENSITIVE_KEYS`] and
    /// scrub any registered secret out of every string.
    pub fn redact_json(&self, value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (k, v) in map.iter_mut() {
                    if is_sensitive_key(k) {
                        if v.is_string() {
                            *v = Value::from(REDACTED);
                        } else {
                            // e.g. an object/array under a sensitive key — redact deeply.
                            self.redact_json(v);
                        }
                    } else {
                        self.redact_json(v);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| self.redact_json(v)),
            Value::String(s) => *s = self.redact_str(s),
            _ => {}
        }
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|k| lower == *k)
}

/// Well-known credential prefixes. A whitespace token that starts with one of these (and
/// is long enough to be a real token) is masked wholesale — covers provider API keys and
/// VCS/CI tokens whose *value* the host never registered (defence in depth).
const SECRET_PREFIXES: &[&str] = &[
    "sk-",          // OpenAI / OpenRouter / Anthropic-style
    "sk_live_",     // Stripe live
    "sk_test_",     // Stripe test
    "pk_live_",     // Stripe publishable (still worth hiding)
    "rk_live_",     // Stripe restricted
    "ghp_",         // GitHub personal access token
    "gho_",         // GitHub OAuth
    "ghu_",         // GitHub user-to-server
    "ghs_",         // GitHub server-to-server
    "ghr_",         // GitHub refresh
    "github_pat_",  // GitHub fine-grained PAT
    "xoxb-",        // Slack bot
    "xoxp-",        // Slack user
    "xoxa-",        // Slack app
    "AKIA",         // AWS access key id
    "ASIA",         // AWS temporary access key id
    "AIza",         // Google API key
    "ya29.",        // Google OAuth access token
    "glpat-",       // GitLab PAT
];

/// Strip surrounding punctuation/quotes so a trailing comma or closing brace doesn't hide
/// a prefix match or leak part of a value.
fn trim_token(tok: &str) -> &str {
    tok.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
}

/// True if `tok` looks like a bare credential token by its prefix.
fn looks_like_secret_token(tok: &str) -> bool {
    let t = trim_token(tok);
    t.len() >= 8 && SECRET_PREFIXES.iter().any(|p| t.starts_with(p))
}

/// Heuristic scrub of a free-text line for secret shapes the host never registered.
/// Operates token-by-token (whitespace-delimited): masks `sensitive_key=value` /
/// `sensitive_key: value` assignments, `Bearer <token>` / `Authorization: <token>`
/// pairs, and any token carrying a [`SECRET_PREFIXES`] prefix. Whitespace is normalised
/// to single spaces in the process — acceptable for a diagnostics view.
pub fn redact_secret_patterns(input: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    // When the previous meaningful token was `bearer` / `authorization`, the *next*
    // token is the credential and gets masked whole.
    let mut mask_next = false;
    for tok in input.split_whitespace() {
        let key = trim_token(tok).to_ascii_lowercase();
        if mask_next {
            // An auth *scheme* word (`Authorization: Bearer <token>`) is kept and the
            // masking deferred to the token that actually carries the credential.
            if key == "bearer" || key == "basic" || key == "token" {
                out.push(tok.to_string());
            } else {
                out.push(REDACTED.to_string());
                mask_next = false;
            }
            continue;
        }
        if key == "bearer" || key == "authorization" {
            out.push(tok.to_string());
            mask_next = true;
            continue;
        }
        if let Some(masked) = redact_assignment(tok) {
            out.push(masked);
        } else if looks_like_secret_token(tok) {
            out.push(REDACTED.to_string());
        } else {
            out.push(tok.to_string());
        }
    }
    out.join(" ")
}

/// If `tok` is a `key=value` or `key:value` assignment whose key is sensitive, return the
/// key and separator with the value replaced by [`REDACTED`]; otherwise `None`.
fn redact_assignment(tok: &str) -> Option<String> {
    // Prefer the first separator so `api_key=sk:abc` masks the whole value.
    let sep = tok.find(['=', ':'])?;
    let (key_raw, rest) = tok.split_at(sep);
    let separator = &rest[..1];
    // Normalise the key: drop quotes and surrounding punctuation.
    let key = key_raw.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    if key.is_empty() || !is_sensitive_key(key) {
        return None;
    }
    Some(format!("{key_raw}{separator}{REDACTED}"))
}

/// Severity of a [`LogRecord`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// One structured log line, tagged with the correlation id that ties a host request →
/// sidecar → response, and the sidecar it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
    pub correlation_id: u64,
    pub sidecar_id: String,
    pub level: LogLevel,
    pub message: String,
}

/// A bounded, thread-safe ring buffer of recent log records (per sidecar). Oldest
/// records are dropped once the capacity is reached, so it can't grow without bound.
pub struct LogCapture {
    capacity: usize,
    records: Mutex<VecDeque<LogRecord>>,
}

impl LogCapture {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            records: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push(&self, record: LogRecord) {
        let mut buf = self.records.lock().unwrap();
        if buf.len() == self.capacity {
            buf.pop_front();
        }
        buf.push_back(record);
    }

    /// A snapshot of the buffered records, oldest first.
    pub fn recent(&self) -> Vec<LogRecord> {
        self.records.lock().unwrap().iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.records.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.lock().unwrap().is_empty()
    }
}

/// One sidecar's entry in a diagnostics bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarDiag {
    pub id: String,
    pub version: String,
    pub running: bool,
}

/// A shareable diagnostics bundle. Build it with [`build_diagnostics`], which runs all
/// log messages through the [`Redactor`] so no secret ships in a support bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostics {
    pub app_version: String,
    pub contract_version: ContractVersion,
    pub sidecars: Vec<SidecarDiag>,
    pub logs: Vec<LogRecord>,
}

/// Assemble a diagnostics bundle, redacting every log message.
pub fn build_diagnostics(
    app_version: impl Into<String>,
    sidecars: Vec<SidecarDiag>,
    logs: Vec<LogRecord>,
    redactor: &Redactor,
) -> Diagnostics {
    let logs = logs
        .into_iter()
        .map(|mut r| {
            r.message = redactor.redact_str(&r.message);
            r
        })
        .collect();
    Diagnostics {
        app_version: app_version.into(),
        contract_version: sidecar_contract::CONTRACT_VERSION,
        sidecars,
        logs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_registered_secrets_from_strings() {
        let r = Redactor::new().with_secret("sk-super-secret");
        assert_eq!(
            r.redact_str("using key sk-super-secret to auth"),
            "using key *** to auth"
        );
    }

    #[test]
    fn an_empty_secret_is_ignored() {
        let r = Redactor::new().with_secret("");
        assert_eq!(r.redact_str("nothing changes"), "nothing changes");
    }

    #[test]
    fn overlapping_secrets_are_redacted_longest_first_without_leaking_a_fragment() {
        // A shorter registered secret that is a prefix of a longer one must not leave the longer
        // one's tail exposed. Registered shortest-first on purpose — the fix must not depend on order.
        let r = Redactor::new().with_secret("SECRET").with_secret("SECRETLONGER");
        let out = r.redact_str("token=SECRETLONGER done");
        assert!(!out.contains("SECRET"), "no secret text remains: {out}");
        assert!(!out.contains("LONGER"), "no fragment of the longer secret leaks: {out}");
        assert_eq!(out, "token=*** done");
    }

    #[test]
    fn redacts_sensitive_json_keys_and_secret_values() {
        let r = Redactor::new().with_secret("leaked-token");
        let mut v = json!({
            "name": "openrouter",
            "value": "sk-abc",
            "nested": { "api_key": "xyz", "note": "contains leaked-token here" },
            "list": ["leaked-token", "fine"]
        });
        r.redact_json(&mut v);
        assert_eq!(v["value"], json!(REDACTED));
        assert_eq!(v["nested"]["api_key"], json!(REDACTED));
        assert_eq!(v["nested"]["note"], json!("contains *** here"));
        assert_eq!(v["list"][0], json!("***"));
        assert_eq!(v["name"], json!("openrouter")); // non-sensitive untouched
    }

    #[test]
    fn log_capture_is_bounded_and_ordered() {
        let cap = LogCapture::new(2);
        for i in 0..3 {
            cap.push(LogRecord {
                correlation_id: i,
                sidecar_id: "s".into(),
                level: LogLevel::Info,
                message: format!("m{i}"),
            });
        }
        let recent = cap.recent();
        assert_eq!(recent.len(), 2, "ring buffer drops the oldest");
        assert_eq!(recent[0].message, "m1");
        assert_eq!(recent[1].message, "m2");
    }

    #[test]
    fn redact_log_line_masks_unregistered_secret_shapes() {
        let r = Redactor::new(); // no registered secrets — pattern scrub must catch these
        // Provider API key by prefix.
        assert_eq!(
            r.redact_log_line("calling provider with key sk-abc123def456ghi789"),
            "calling provider with key ***"
        );
        // Bearer token.
        assert_eq!(
            r.redact_log_line("GET /v1 Authorization: Bearer eyJhbGciOi.payload.sig"),
            "GET /v1 Authorization: Bearer ***"
        );
        // Sensitive key=value assignment.
        assert_eq!(
            r.redact_log_line("connecting api_key=super-secret-value endpoint=api.example.com"),
            "connecting api_key=*** endpoint=api.example.com"
        );
        // Sensitive key: value assignment (colon form).
        assert_eq!(
            r.redact_log_line("auth token:abcd1234deadbeef done"),
            "auth token:*** done"
        );
        // GitHub PAT by prefix, wrapped in punctuation (the whole token is masked).
        assert_eq!(
            r.redact_log_line("token (ghp_0123456789abcdef0123456789abcdef0123)."),
            "token ***"
        );
    }

    #[test]
    fn redact_log_line_leaves_ordinary_text_alone() {
        let r = Redactor::new();
        let line = "listing /home/user/docs took 12ms, 42 entries";
        assert_eq!(r.redact_log_line(line), line);
    }

    #[test]
    fn redact_log_line_still_scrubs_registered_secrets() {
        let r = Redactor::new().with_secret("hunter2");
        assert_eq!(
            r.redact_log_line("login failed for user bob with hunter2"),
            "login failed for user bob with ***"
        );
    }

    #[test]
    fn diagnostics_bundle_redacts_log_messages() {
        let r = Redactor::new().with_secret("hunter2");
        let logs = vec![LogRecord {
            correlation_id: 1,
            sidecar_id: "ai-console".into(),
            level: LogLevel::Error,
            message: "login failed with password hunter2".into(),
        }];
        let diag = build_diagnostics("0.11.0", vec![], logs, &r);
        assert_eq!(diag.logs[0].message, "login failed with password ***");
        assert_eq!(diag.app_version, "0.11.0");
    }
}
