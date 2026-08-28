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
///
/// Both components come from `console_temp_dir` (CPE-1975) rather than being spelled here — this was
/// one of the three sites that each built the same fixed path by hand, and the reason two of them got
/// hardened and one did not was that nobody had them in one place.
pub fn log_path() -> PathBuf {
    crate::console_temp_dir::console_temp_dir().join(crate::console_temp_dir::DIAG_LOG_NAME)
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Append one timestamped line, and echo it to stderr. Best-effort — never panics, never blocks I/O.
///
/// **Gated: it does nothing at all unless [`enabled()`] — one of four env vars — is true.** Stated
/// here because the `eprintln!` below reads as an unconditional stderr echo and is only unconditional
/// *relative to the file writes after it*. CPE-1975 round 2 relied on that misreading to report a
/// security-relevant refusal from `session_supervisor::discover_or_spawn`, which runs in the
/// **console** process where none of the four vars is set by default (`CPE_AICONSOLE_DIAG` is set by
/// `run_session_daemon()` in the *daemon* process; `CPE_AICONSOLE_SESSION_DAEMON_ADDR` only on the
/// host-injected path, which does not call `discover_or_spawn`) — so the report went nowhere.
///
/// If a message **must** be seen regardless of diagnostics, write it to the real stderr handle
/// yourself and call this in addition, not instead. That is what that call site now does.
pub fn trace(component: &str, msg: &str) {
    if !enabled() {
        return;
    }
    let line = format!("{} pid={} {}: {}", now_ms(), std::process::id(), component, msg);
    eprintln!("[cpe-diag] {line}");
    let path = log_path();
    // CPE-1975. This used to be `let _ = std::fs::create_dir_all(dir);` — which walks a junction or
    // symlink planted at `<temp>/cpe-ai-console` and then appends this log, which carries session
    // ids, pids and byte counts, into whatever directory the attacker chose.
    //
    // The failure must be an EARLY RETURN, not the old `let _ =`. Ignoring the error and falling
    // through to the open would defeat the whole change: `OpenOptions::create(true)` resolves the
    // path itself, so it writes through the link with or without our `create_dir`. The two refusals
    // below are what stop the write; the stderr echo above has already happened, so a refused trace
    // is not a silent one.
    let Some(dir) = path.parent() else { return };
    if crate::console_temp_dir::ensure_console_dir_at(dir).is_err() {
        return;
    }
    // And the log file itself must not be a link inside an otherwise-genuine directory.
    if !crate::console_temp_dir::regular_file_or_absent(&path) {
        return;
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

    /// **`trace` is OFF by default, so it is not a channel for anything that must be seen.**
    ///
    /// This is the fact CPE-1975 round 2 got wrong, and it got it wrong in the direction that
    /// matters: it routed a security-relevant refusal ("something is planted at the rendezvous
    /// path", from `session_supervisor::discover_or_spawn`) through `trace` alone, on the reasoning
    /// that `trace` "echoes to stderr unconditionally". The `eprintln!` is unconditional only
    /// *relative to the file writes after it*; `trace` itself returns early unless [`enabled()`].
    ///
    /// `discover_or_spawn` runs in the **console** process, where none of the four vars is set by
    /// default — `CPE_AICONSOLE_DIAG` is set by `run_session_daemon()` in the *daemon* process, and
    /// `CPE_AICONSOLE_SESSION_DAEMON_ADDR` only on the host-injected path, which uses
    /// `SessionDaemonHandle::external` instead. So the report reached nobody.
    ///
    /// ## What this test does and does NOT guard — corrected in round 4, because it claimed both
    ///
    /// Round 3 wrote here that this pins the fact "so that a future edit routing a must-see message
    /// back through `trace` alone has a red test standing next to it". **That was false and was
    /// measured false:** deleting the ungated `writeln!` from `session_supervisor::discover_or_spawn`
    /// and leaving its `trace` call alone left the crate at **423 passed / 0 failed**. This test
    /// asserts a property of `trace`; nothing structurally connects it to any call site, so it could
    /// not fire for the regression it named — CLAUDE.md's *"do not name a backstop without checking
    /// it can fire; one that structurally cannot fire is worse than none, because it reads as one."*
    ///
    /// **The call site is guarded by `src/lib/consoleRefusalReport.test.ts`**, which derives the
    /// shape of that error arm from the Rust source (comments stripped first) and was red-proofed
    /// against exactly that deletion. This test's job is only the narrower fact its name states.
    ///
    /// Round 3 also shipped two weak legs, both now gone: an `assert!(!enabled())` that merely
    /// restated one already made twenty lines up in `a_byte_trace_is_inert_when_disabled`, and an
    /// `assert_eq!(log_path().exists(), before)` that **compared existence to existence** — vacuous
    /// wherever the log already exists, which in this crate's own suite is near-guaranteed because
    /// `tests/session_supervisor.rs` and `tests/session_engine_daemon.rs` spawn the real
    /// `--session-daemon`, and it sets `CPE_AICONSOLE_DIAG` on itself and appends to that same file.
    /// With the gate sabotaged (`if false && !enabled()`) the trace wrote 66 bytes to the real log
    /// and the test still passed.
    ///
    /// So the assertion is now on **content, via a token no other writer can produce** — immune both
    /// to the file pre-existing and to a concurrent daemon appending to it, which a length or mtime
    /// comparison would not be.
    ///
    /// If this ever fails because a fifth env var was added, or because the harness sets one, the
    /// conclusion is not "relax the test", it is "`trace` is still not guaranteed on, so must-see
    /// messages still need the real stderr handle".
    ///
    /// ## CPE-1929 pair, re-run on the repaired test (Windows, 2026-08-28)
    ///
    /// Round 3's version failed **both** legs green, which is this repo's definition of unreachable.
    /// The repaired version:
    ///
    /// * **gate made to lie** (`if false && !enabled()` in [`trace`], i.e. no gate at all) →
    ///   **RED**, `cargo test --locked --no-fail-fast --lib` 395 passed / **1 failed**, this test.
    ///   Round 3's version was **GREEN** under the identical sabotage — the vacuous
    ///   existence-vs-existence compare — so this is the leg that was bought.
    /// * the **other** half of the pair, "delete the `writeln!` at the call site", is not this
    ///   test's to catch and is not claimed here; `src/lib/consoleRefusalReport.test.ts` owns it and
    ///   was red-proofed against exactly that deletion.
    ///
    /// Windows-measured only, like every other pair in this ticket.
    #[test]
    fn a_disabled_trace_writes_nothing_to_the_log() {
        let token = format!(
            "CPE-1975-must-go-nowhere-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        trace("test", &token);
        let contents = std::fs::read_to_string(log_path()).unwrap_or_default();
        assert!(
            !contents.contains(&token),
            "a disabled `trace` wrote to {} — so `trace` is NOT inert by default, and every place \
             that relies on it being a real channel (or on it being a silent one) needs re-checking. \
             See this test's doc and CPE-1975.",
            log_path().display()
        );
    }
}
