//! Session-daemon I/O tracer (CPE-309) — an opt-in diagnostic that pinpoints **where PTY output
//! stops** on the daemon reattach path, so the one manual GUI run can be evidence-based instead of
//! guesswork (the whole daemon path has repeatedly shipped a "black terminal / no output" that no
//! headless test reproduced).
//!
//! It brackets every hop of the daemon transport with byte counters:
//!   daemon: pty[<id>]      — bytes read from the PTY inside the daemon process
//!   client: recv           — output/replay events arriving at the sidecar's `SessionClient`
//!   console: pump[<id>]     — bytes the console consumed and pushed toward the live WebSocket
//! Wherever the "first bytes" line is missing, that is the hop that broke.
//!
//! **Inert unless the daemon path is active** (`enabled()` below). Unit tests set none of these env
//! vars, so the 184-test session subsystem traces nothing and is unaffected. Writes are best-effort:
//! a failed log line never disturbs the I/O it observes. Lines go both to a temp log file (for the
//! user to read after the run) and to stderr (which the host may capture into the Diagnostics panel).

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Tracing is on only when the session-daemon reattach path is in use: the user's opt-in flag
/// (`CPE_AICONSOLE_DAEMON`), the address the host injects into the sidecar
/// (`CPE_AICONSOLE_SESSION_DAEMON_ADDR`), the legacy engine flag, or an explicit `CPE_AICONSOLE_DIAG`
/// (which the daemon process sets on itself). Tests set none of these ⇒ inert.
pub fn enabled() -> bool {
    ["CPE_AICONSOLE_DIAG", "CPE_AICONSOLE_DAEMON", "CPE_AICONSOLE_SESSION_DAEMON", "CPE_AICONSOLE_SESSION_DAEMON_ADDR"]
        .iter()
        .any(|k| std::env::var_os(k).is_some())
}

/// The trace log file: `<temp>/cpe-ai-console/session-diag.log`.
pub fn log_path() -> PathBuf {
    std::env::temp_dir().join("cpe-ai-console").join("session-diag.log")
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Append one timestamped line, and echo it to stderr. Best-effort — never panics, never blocks I/O.
pub fn trace(component: &str, msg: &str) {
    if !enabled() {
        return;
    }
    let line = format!("{} pid={} {}: {}", now_ms(), std::process::id(), component, msg);
    eprintln!("[cpe-diag] {line}");
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

/// A per-hop cumulative byte counter. Emits a line on the **first** byte (the load-bearing signal:
/// did *anything* flow at this hop?), then a throttled heartbeat every ~8 KiB, and a final total.
/// Cheap and lock-free — one lives per session-thread at each instrumented hop.
pub struct ByteTrace {
    component: &'static str,
    label: String,
    total: u64,
    last_logged: u64,
    started: bool,
}

const HEARTBEAT: u64 = 8 * 1024;

impl ByteTrace {
    pub fn new(component: &'static str, label: impl Into<String>) -> Self {
        Self { component, label: label.into(), total: 0, last_logged: 0, started: false }
    }

    /// Record `n` bytes flowing through this hop.
    pub fn add(&mut self, n: usize) {
        if !enabled() || n == 0 {
            return;
        }
        self.total += n as u64;
        if !self.started {
            self.started = true;
            self.last_logged = self.total;
            trace(self.component, &format!("{} FIRST bytes (+{n}, total {})", self.label, self.total));
        } else if self.total - self.last_logged >= HEARTBEAT {
            self.last_logged = self.total;
            trace(self.component, &format!("{} total {} bytes", self.label, self.total));
        }
    }

    /// Note a terminal event for this hop (EOF/exit/close), reporting the final byte total.
    pub fn end(&self, what: &str) {
        trace(self.component, &format!("{} {what} (final total {} bytes)", self.label, self.total));
    }
}

/// Lightweight, id-less checkpoint for the client demux (which sees many sessions on one thread):
/// log the first N stream events and then every 64th, so we can tell whether output events reach the
/// sidecar at all without threading per-session state through the hot path.
static CLIENT_EVENTS: AtomicU64 = AtomicU64::new(0);

pub fn note_client_event(id: &str, ev: &str, len: usize) {
    if !enabled() {
        return;
    }
    let n = CLIENT_EVENTS.fetch_add(1, Ordering::Relaxed);
    if n < 8 || n.is_multiple_of(64) {
        trace("client", &format!("recv {ev}[{id}] len={len} (#{n})"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_byte_trace_is_inert_when_disabled() {
        // With no daemon-path env var set (the default in tests), enabled() is false, and add()/end()
        // must not panic or write anything. NB: we deliberately do NOT mutate env vars here — they are
        // process-global and the test harness runs tests in parallel, so flipping one on would make
        // concurrent session tests start tracing. The env→enabled wiring is trivial and read-only.
        assert!(!enabled(), "diag must be off unless a daemon-path env var is set");
        let mut bt = ByteTrace::new("test", "x");
        bt.add(100);
        bt.add(0);
        bt.end("eof");
        note_client_event("s1", "output", 10);
    }

    #[test]
    fn log_path_is_under_the_daemon_temp_dir() {
        assert!(log_path().ends_with("cpe-ai-console/session-diag.log")
            || log_path().ends_with("cpe-ai-console\\session-diag.log"));
    }
}
