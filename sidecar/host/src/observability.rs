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
        let mut out = input.to_string();
        for secret in &self.secrets {
            if out.contains(secret.as_str()) {
                out = out.replace(secret.as_str(), REDACTED);
            }
        }
        out
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
