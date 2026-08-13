//! Embedded terminal PTY backend (CPE-1242, epic CPE-714): the session type + a live-session registry
//! backing the terminal dock's `open_pty`/`write_pty`/`resize_pty`/`close_pty` commands.
//!
//! `PtySession`/`PtyLaunch`/`default_shell`/`shell_command` **mirror** the proven
//! `sidecar/ai-console/src/pty.rs` (`portable-pty` 0.8, already a workspace dep — no new backend
//! dependency) byte-for-byte in shape, minus the sidecar's npm/pip shim resolution (`cli_command`),
//! which a plain interactive shell doesn't need.
//!
//! This module owns OS processes/handles but stays Tauri-free on purpose — like the sidecar's own
//! `pty.rs` and `session_engine.rs`, it's unit-testable with real spawns and no running app. The
//! `ipc::Channel` transport, the `AppHandle`-based self-clean, and the base64 wire encoding are thin
//! glue that lives in the `#[tauri::command]`s in `lib.rs` (CLAUDE.md: "any `ipc::Channel` stays in the
//! app adapter").
//!
//! Home crate: `src-tauri`, not `cpe-server`. A PTY session spawns OS processes and holds OS handles —
//! adapter territory, like the sidecar host — so it lives beside the app rather than in the Tauri-free
//! domain crate (see the PR description for the full rationale).
//!
//! Lifecycle hardening (CPE-1244, follow-up to the CPE-1242 review): [`PtySession::kill`] does a final
//! `wait()` after the kill so a SIGKILL-escalated child is always reaped (no zombie), [`PtySession`] has
//! a belt-and-braces [`Drop`] impl that kills+reaps too, and [`PtyRegistry::close_all`] sweeps every live
//! session on app quit (wired to `RunEvent::Exit` in `lib.rs::run()`) so terminal tabs left open never
//! orphan their shells. The sidecar's own `ai-console/src/pty.rs` has the same two gaps and should get
//! matching treatment in a separate PR — not done here (out of scope for this app-side ticket).

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

/// A running shell in a pseudo-terminal.
pub struct PtySession {
    /// `Some` while the session is live. `kill` drops it, which closes the ConPTY so the output reader
    /// EOFs — the only way a Windows ConPTY reader ends when the session is *held* by the registry
    /// rather than owned+dropped by the caller (mirrors `sidecar/ai-console/src/pty.rs`, CPE-574; on
    /// Unix the reader EOFs on child exit already).
    master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

/// How to launch a session.
pub struct PtyLaunch {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    /// Extra environment for the child only.
    pub env: BTreeMap<String, String>,
    pub rows: u16,
    pub cols: u16,
}

impl PtySession {
    /// Spawn `launch` in a fresh PTY.
    pub fn spawn(launch: &PtyLaunch) -> Result<PtySession, String> {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: launch.rows.max(1),
                cols: launch.cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("openpty: {e}"))?;

        let mut cmd = CommandBuilder::new(&launch.program);
        cmd.args(&launch.args);
        if let Some(cwd) = &launch.cwd {
            cmd.cwd(cwd);
        }
        for (k, v) in &launch.env {
            cmd.env(k, v);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("spawn {}: {e}", launch.program))?;
        // Drop the slave so the master sees EOF when the child exits.
        drop(pair.slave);

        Ok(PtySession { master: Some(pair.master), child })
    }

    /// Borrow the live master, or error if the session has been killed (master dropped).
    fn master(&self) -> Result<&(dyn portable_pty::MasterPty + Send), String> {
        self.master.as_deref().ok_or_else(|| "session terminated".to_string())
    }

    /// A reader for the terminal's output stream (clone of the master).
    pub fn reader(&self) -> Result<Box<dyn Read + Send>, String> {
        self.master()?.try_clone_reader().map_err(|e| e.to_string())
    }

    /// A writer for the terminal's input stream. Take this **once** per session and hold it — a fresh
    /// `take_writer()` per call detaches ownership and would close stdin after the first write (the
    /// sidecar's `session_daemon.rs` hit exactly this bug with a per-keystroke `take_writer()`).
    pub fn writer(&self) -> Result<Box<dyn Write + Send>, String> {
        self.master()?.take_writer().map_err(|e| e.to_string())
    }

    /// Resize the terminal (on panel/window resize).
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.master()?
            .resize(PtySize { rows: rows.max(1), cols: cols.max(1), pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())
    }

    /// True while the child is still running.
    #[cfg(test)]
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Non-blocking exit check: `Some(code)` if the child has exited, `None` if it's still running.
    /// Test-only for now (proves `kill()` reaps the child rather than leaking it); production code has
    /// no liveness poller yet.
    #[cfg(test)]
    pub fn try_wait(&mut self) -> Option<u32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.exit_code()),
            _ => None,
        }
    }

    /// Terminate the session: kill the child and **close the ConPTY** (drop the master), so the output
    /// reader EOFs even when the session is held by the registry rather than the caller. Idempotent.
    ///
    /// CPE-1244: portable-pty 0.8.1's `ChildKiller` (the unix `impl ChildKiller for std::process::Child`
    /// in its `lib.rs`) sends SIGHUP, polls `try_wait` ~5×50ms for a graceful exit, then — if the child
    /// ignored SIGHUP — escalates to a bare `std::process::Child::kill()` (SIGKILL) with **no follow-up
    /// wait**. `Child::kill()` only *sends* the signal; nothing reaps the now-dead child, so a
    /// SIGHUP-ignoring child would be SIGKILL'd but left a transient zombie until something else waits
    /// on it. The explicit `self.child.wait()` below closes that gap: it's a no-op/instant when the
    /// grace-period polling already reaped the child (`std::process::Child` caches the exit status once
    /// observed), and it's the actual reap when escalation was needed.
    pub fn kill(&mut self) -> Result<(), String> {
        let r = self.child.kill().map_err(|e| e.to_string());
        let _ = self.child.wait(); // reap unconditionally -- see CPE-1244 doc comment above
        self.master = None; // drop the master -> close the ConPTY -> the output reader EOFs
        r
    }
}

impl Drop for PtySession {
    /// Belt-and-braces (CPE-1244): every current caller already calls `kill()` before a session is
    /// dropped (`PtyRegistry::close`/`close_all`, and the tests below), so in practice this is a no-op.
    /// But it costs nothing to make dropping a live session *always* kill + reap the child too, so a
    /// future code path that forgets the explicit `kill()` can never leak a running shell or a zombie.
    /// Idempotent/cheap on an already-killed session: `child.kill()` on a dead process errors (discarded)
    /// and `child.wait()` returns the cached exit status instantly.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The default interactive shell for the current OS: `%COMSPEC%` (usually `cmd.exe`) on Windows,
/// `$SHELL` (falling back to `/bin/bash`) elsewhere. No args — an interactive shell needs none.
pub fn default_shell() -> (String, Vec<String>) {
    if cfg!(windows) {
        (std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()), Vec::new())
    } else {
        (std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string()), Vec::new())
    }
}

/// Resolve the shell dock's `open_pty` should launch (CPE-1243's shell picker): `shell` is the frontend's
/// choice — a program name/path (e.g. `"powershell.exe"`, `"/bin/zsh"`) with no args, since an interactive
/// shell needs none — or `None`/blank to fall back to [`default_shell`]. A tiny pure function so the
/// picker's override path is unit-tested here rather than only exercised indirectly through the
/// `#[tauri::command]` dispatcher in `lib.rs`.
pub fn resolve_shell(shell: Option<String>) -> (String, Vec<String>) {
    match shell.filter(|s| !s.trim().is_empty()) {
        Some(program) => (program, Vec::new()),
        None => default_shell(),
    }
}

/// The launch command + args for an OS shell running a one-off inline command — used to exercise the
/// PTY cross-platform in tests (mirrors `sidecar/ai-console/src/pty.rs::shell_command`). Test-only:
/// production code always runs the interactive `default_shell()`.
#[cfg(test)]
fn shell_command(inline: &str) -> (String, Vec<String>) {
    if cfg!(target_os = "windows") {
        ("cmd".to_string(), vec!["/c".to_string(), inline.to_string()])
    } else {
        ("sh".to_string(), vec!["-c".to_string(), inline.to_string()])
    }
}

/// One live registry entry: the session plus its **held** input writer (see [`PtySession::writer`]'s
/// docs on why it must be taken once and reused).
struct PtyHandle {
    session: Mutex<PtySession>,
    writer: Mutex<Box<dyn Write + Send>>,
}

/// The registry of live PTY sessions for the terminal dock, keyed by a backend-generated id. Cheaply
/// cloneable (an `Arc` around the map), so a Tauri command can clone the handle out of managed state and
/// move it into a `spawn_blocking`/background thread — mirrors `cpe_server::index_service::IndexService`.
///
/// The map **is** the live-session count: empty means zero PTYs and zero background threads. `close`
/// (and the reader thread's own EOF self-clean in `lib.rs`, for a child that exits on its own) always
/// removes the entry first, so a closed — or already-dead — panel never lingers as a "live" session
/// (CPE-1242 DoD: no PTY/background cost when nothing's open).
#[derive(Clone, Default)]
pub struct PtyRegistry {
    next_id: Arc<AtomicU64>,
    sessions: Arc<Mutex<HashMap<u64, Arc<PtyHandle>>>>,
}

impl PtyRegistry {
    /// Register a freshly spawned session (with its writer already taken once) under a new id.
    pub fn insert(&self, session: PtySession, writer: Box<dyn Write + Send>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.sessions
            .lock()
            .unwrap()
            .insert(id, Arc::new(PtyHandle { session: Mutex::new(session), writer: Mutex::new(writer) }));
        id
    }

    fn get(&self, id: u64) -> Option<Arc<PtyHandle>> {
        self.sessions.lock().unwrap().get(&id).cloned()
    }

    /// Remove `id`'s entry (idempotent — `None` if already gone), so it stops counting as live. The
    /// caller is still holding the returned handle and finishes killing it after the registry no longer
    /// lists it. Private: `close` is the public way to do this (remove + kill together).
    fn remove(&self, id: u64) -> Option<Arc<PtyHandle>> {
        self.sessions.lock().unwrap().remove(&id)
    }

    /// Write bytes to `id`'s held input writer.
    pub fn write(&self, id: u64, bytes: &[u8]) -> Result<(), String> {
        let handle = self.get(id).ok_or_else(|| format!("no such pty session '{id}'"))?;
        let mut w = handle.writer.lock().map_err(|_| "writer poisoned".to_string())?;
        w.write_all(bytes).map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())
    }

    /// Resize `id`'s terminal.
    pub fn resize(&self, id: u64, rows: u16, cols: u16) -> Result<(), String> {
        let handle = self.get(id).ok_or_else(|| format!("no such pty session '{id}'"))?;
        let session = handle.session.lock().map_err(|_| "session poisoned".to_string())?;
        session.resize(rows, cols)
    }

    /// Close `id`: remove it from the registry, then kill the child + free the PTY. Idempotent —
    /// closing an unknown/already-closed id is a no-op success.
    pub fn close(&self, id: u64) -> Result<(), String> {
        let Some(handle) = self.remove(id) else { return Ok(()) };
        let mut session = handle.session.lock().map_err(|_| "session poisoned".to_string())?;
        session.kill()
    }

    /// Number of live sessions. Test/diagnostics only — production code should never need to enumerate
    /// the registry; each id is handed back to its own caller by `open`.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }

    /// App-quit sweep (CPE-1244): close and kill **every** live session, leaving the registry empty.
    /// Wired to Tauri's `RunEvent::Exit` in `lib.rs::run()` so a user quitting with terminal tabs still
    /// open never orphans those shell processes. Idempotent (an empty/already-swept registry is a
    /// no-op) and best-effort per session — one session's kill failing doesn't stop the sweep of the
    /// rest. Drains the map first (mirroring `close`'s remove-then-kill order) so nothing can be handed
    /// out as "live" mid-sweep.
    pub fn close_all(&self) {
        let handles: Vec<Arc<PtyHandle>> = self.sessions.lock().unwrap().drain().map(|(_, h)| h).collect();
        for handle in handles {
            if let Ok(mut session) = handle.session.lock() {
                let _ = session.kill();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::{Duration, Instant};

    /// Drain a session's output on a background thread until `marker` appears or the deadline passes.
    /// Timed because a Windows ConPTY master may never signal EOF on its own, so a plain
    /// `read_to_string` would block forever (mirrors `sidecar/ai-console/src/pty.rs`'s own helper).
    fn drain_until(session: &PtySession, marker: &str, timeout: Duration) -> String {
        let reader = session.reader().unwrap();
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        let mut out = String::new();
        let deadline = Instant::now() + timeout;
        while !out.contains(marker) {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else { break };
            match rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
                Ok(chunk) => out.push_str(&String::from_utf8_lossy(&chunk)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        out
    }

    fn spawn_inline(inline: &str) -> PtySession {
        let (program, args) = shell_command(inline);
        PtySession::spawn(&PtyLaunch {
            program,
            args,
            cwd: None,
            env: BTreeMap::new(),
            rows: 24,
            cols: 80,
        })
        .expect("spawn pty")
    }

    /// OS-level "is this pid still around" check (CPE-1244) — mirrors CPE-1242's manual UAT `tasklist`
    /// check, but as an automated cross-platform test helper: `tasklist` on Windows, `ps -p` elsewhere
    /// (works on both Linux and macOS CI runners, unlike a Linux-only `/proc/<pid>` check). Used to prove
    /// `kill()`/`close_all()` actually terminate the OS process, not just close our handle to it.
    fn pid_is_alive(pid: u32) -> bool {
        #[cfg(windows)]
        {
            let out = std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/NH"])
                .output();
            match out {
                Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()),
                Err(_) => false,
            }
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("ps")
                .args(["-p", &pid.to_string()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
    }

    #[test]
    fn runs_a_command_in_a_real_pty_and_streams_output() {
        let mut session = spawn_inline("echo pty-works");
        let out = drain_until(&session, "pty-works", Duration::from_secs(10));
        assert!(out.contains("pty-works"), "got: {out:?}");
        let _ = session.kill();
    }

    #[test]
    fn cwd_and_env_are_applied_to_the_child() {
        let dir = std::env::temp_dir();
        let inline = if cfg!(windows) { "echo %CPE_TEST_VAR% & cd" } else { "echo $CPE_TEST_VAR; pwd" };
        let (program, args) = shell_command(inline);
        let mut env = BTreeMap::new();
        env.insert("CPE_TEST_VAR".to_string(), "injected-value".to_string());
        let mut session = PtySession::spawn(&PtyLaunch {
            program,
            args,
            cwd: Some(dir.to_string_lossy().into_owned()),
            env,
            rows: 24,
            cols: 80,
        })
        .expect("spawn pty");
        let out = drain_until(&session, "injected-value", Duration::from_secs(10));
        assert!(out.contains("injected-value"), "got: {out:?}");
        let _ = session.kill();
    }

    #[test]
    fn write_reaches_an_interactive_shell_and_its_echo_streams_back() {
        // A real interactive shell (no `-c`/`/c` one-shot command) so a written line is executed and its
        // output read back, exercising `writer()` the same way `open_pty`/`write_pty` will.
        let (program, args) = default_shell();
        let mut session = PtySession::spawn(&PtyLaunch {
            program,
            args,
            cwd: None,
            env: BTreeMap::new(),
            rows: 24,
            cols: 80,
        })
        .expect("spawn default shell");
        let mut writer = session.writer().expect("writer");
        let line = if cfg!(windows) { "echo pty-write-works\r\n" } else { "echo pty-write-works\n" };
        writer.write_all(line.as_bytes()).expect("write");
        writer.flush().expect("flush");
        let out = drain_until(&session, "pty-write-works", Duration::from_secs(10));
        assert!(out.contains("pty-write-works"), "got: {out:?}");
        let _ = session.kill();
    }

    #[test]
    fn resize_and_liveness_then_kill_reaps_the_child() {
        // A command that stays alive briefly so we can resize + observe liveness before killing it.
        let inline = if cfg!(windows) { "ping -n 4 127.0.0.1 >NUL" } else { "sleep 2" };
        let mut session = spawn_inline(inline);
        assert!(session.resize(40, 120).is_ok());
        assert!(session.is_running());
        session.kill().unwrap();
        // `kill` reaps the child: waiting on it now returns promptly instead of hanging for the full
        // sleep, proving there's no leaked/still-running process behind a closed session.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if session.try_wait().is_some() {
                break;
            }
            assert!(Instant::now() < deadline, "child was not reaped after kill()");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn kill_reaps_the_child_synchronously() {
        // CPE-1244: `kill()` now does its own final `wait()`, so the reap should be observable
        // *immediately* after `kill()` returns -- no polling loop needed (unlike the test above, which
        // predates this fix and tolerates a delayed reap). A long-lived command so we're sure it's still
        // running right before the kill.
        let inline = if cfg!(windows) { "ping -n 10 127.0.0.1 >NUL" } else { "sleep 10" };
        let mut session = spawn_inline(inline);
        assert!(session.is_running(), "child should still be running before kill()");
        let pid = session.child.process_id().expect("child should report a pid");
        assert!(pid_is_alive(pid), "child process should be alive before kill()");

        session.kill().unwrap();

        // No sleep/poll: `try_wait()` must report the child as no longer running immediately after
        // `kill()` returns. `try_wait()` itself is trustworthy here -- it replays `portable-pty`'s
        // Windows `Child::try_wait()`, backed by `GetExitCodeProcess`, so `Some` means the OS has really
        // recorded an exit code, not just that we think it should have. But don't read this assertion's
        // strictness as proof that `kill()`'s explicit follow-up `self.child.wait()` (the CPE-1244 fix,
        // which closes a reap gap in `portable-pty`'s *Unix* `ChildKiller` escalation path) is what makes
        // it pass: a CPE-1707 probe that disabled that follow-up `wait()` still passed 100/100 runs here,
        // because `TerminateProcess` against a simple console child resolves fast enough that
        // `GetExitCodeProcess` already shows the exit by the time this line runs, with or without the
        // explicit reap. So on Windows this assertion is strong, reliable evidence against the crudest
        // failure mode -- `kill()` failing to actually deliver the kill at all, which would leave the
        // child running and this line red, every time -- not evidence that the reap step specifically is
        // exercised. Never observed to flake either way -- not in CI history, and not across 940+ manual
        // loop iterations run for CPE-1707 (150 sequential baseline, 150 sequential + 320 parallel under
        // a 32-thread CPU load, and 320 parallel under 128 threads oversubscribing this dev box's 32
        // cores 4x).
        assert!(session.try_wait().is_some(), "child was not reaped synchronously by kill()");

        // CPE-1707: deliberately NOT asserting `!pid_is_alive(pid)` here anymore (the test was renamed
        // off "...with_no_zombie_left_behind" for the same reason -- a name shouldn't promise a check
        // the body doesn't make). That assertion is what actually flaked on Windows CI (PR #887:
        // panicked at this test, "OS process should be gone right after kill()" -- pasted verbatim from
        // the real job log, not reproduced by rewording). `pid_is_alive()` shells out to `tasklist`,
        // which enumerates the OS process table -- a different, weaker guarantee than the `try_wait()`
        // check above. Two reasons that table can still list `pid` the instant after our own `wait()`
        // returns, neither of which is a bug in `kill()`:
        //   1. `session.child` (this test's own handle to the process) stays open for the rest of the
        //      test -- `kill()` doesn't close it, only `wait()`s on it -- and Windows keeps a process
        //      object (and its enumerable PID) alive as long as *any* handle references it, regardless
        //      of whether the process has already finished executing.
        //   2. `tasklist` is itself a freshly spawned external process; its own startup/snapshot latency
        //      is unbounded and widens under exactly the CPU contention CI runners see (this sprint has
        //      had four PRs building at once for hours).
        // Nothing in `kill()`'s contract, or in `portable-pty`'s, promises an external enumeration tool
        // observes termination synchronously with our own `wait()`. Asserting it made the test evidence
        // of scheduler/tool timing rather than of `kill()`'s correctness -- and per CPE-1679, the fix
        // for that is to assert what's actually guaranteed, not to wrap the flaky check in a sleep/poll
        // that would hide the same non-guarantee behind a delay instead of removing it.
    }

    // --- PtyRegistry: the id-keyed session bookkeeping that backs open/write/resize/close_pty --------

    #[test]
    fn registry_open_write_resize_close_round_trip() {
        let registry = PtyRegistry::default();
        let (program, args) = default_shell();
        let session = PtySession::spawn(&PtyLaunch {
            program,
            args,
            cwd: None,
            env: BTreeMap::new(),
            rows: 24,
            cols: 80,
        })
        .expect("spawn default shell");
        let reader = session.reader().expect("reader");
        let writer = session.writer().expect("writer");
        let id = registry.insert(session, writer);
        assert_eq!(registry.len(), 1);

        // write() through the registry reaches the shell, and the reader taken before insert() sees it.
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        let line = if cfg!(windows) { "echo registry-write-works\r\n" } else { "echo registry-write-works\n" };
        registry.write(id, line.as_bytes()).expect("registry write");
        let mut out = String::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !out.contains("registry-write-works") && Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(chunk) => out.push_str(&String::from_utf8_lossy(&chunk)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        assert!(out.contains("registry-write-works"), "got: {out:?}");

        assert!(registry.resize(id, 30, 100).is_ok());

        // close() removes the entry (no live session left) AND kills the child.
        registry.close(id).expect("close");
        assert_eq!(registry.len(), 0);
        assert!(registry.write(id, b"echo too-late\n").is_err(), "write after close should fail");
        assert!(registry.resize(id, 10, 10).is_err(), "resize after close should fail");

        // Idempotent: closing again is a no-op success, not an error.
        assert!(registry.close(id).is_ok());
    }

    #[test]
    fn unknown_session_id_errors_instead_of_panicking() {
        let registry = PtyRegistry::default();
        assert!(registry.write(999, b"x").is_err());
        assert!(registry.resize(999, 10, 10).is_err());
        assert!(registry.close(999).is_ok()); // closing an unknown id is a no-op, not an error
    }

    #[test]
    fn close_all_kills_every_session_and_leaves_no_process_behind() {
        // CPE-1244: the app-quit sweep. Two independent long-lived sessions, so the sweep is proven to
        // kill *every* live session, not just the first one it finds.
        let registry = PtyRegistry::default();
        let inline = if cfg!(windows) { "ping -n 10 127.0.0.1 >NUL" } else { "sleep 10" };

        let mut ids_and_pids = Vec::new();
        for _ in 0..2 {
            let mut session = spawn_inline(inline);
            assert!(session.is_running());
            let pid = session.child.process_id().expect("child should report a pid");
            assert!(pid_is_alive(pid), "child should be alive before close_all()");
            let writer = session.writer().expect("writer");
            let id = registry.insert(session, writer);
            ids_and_pids.push((id, pid));
        }
        assert_eq!(registry.len(), 2);

        registry.close_all();

        assert_eq!(registry.len(), 0, "close_all() should empty the registry");
        for (id, pid) in ids_and_pids {
            assert!(
                registry.write(id, b"echo too-late\n").is_err(),
                "session {id} should be gone from the registry after close_all()"
            );
            // The OS-level check: not just "the registry forgot about it" but "the process is actually
            // dead" -- proving close_all() really kills (not merely drops/forgets) each session.
            assert!(!pid_is_alive(pid), "OS process {pid} should be gone after close_all()");
        }
    }

    #[test]
    fn close_all_on_an_empty_registry_is_a_no_op() {
        let registry = PtyRegistry::default();
        registry.close_all(); // must not panic
        assert_eq!(registry.len(), 0);
    }

    // --- resolve_shell: the CPE-1243 shell-picker override path ----------------------------------------

    #[test]
    fn resolve_shell_none_or_blank_falls_back_to_the_os_default() {
        assert_eq!(resolve_shell(None), default_shell());
        assert_eq!(resolve_shell(Some("".to_string())), default_shell());
        assert_eq!(resolve_shell(Some("   ".to_string())), default_shell());
    }

    #[test]
    fn resolve_shell_uses_the_frontend_s_choice_verbatim_with_no_args() {
        let (program, args) = resolve_shell(Some("powershell.exe".to_string()));
        assert_eq!(program, "powershell.exe");
        assert!(args.is_empty());
    }

    #[test]
    fn a_picked_shell_actually_runs_in_a_real_pty() {
        // Exercise resolve_shell's output through a real PtySession — the picker is only useful if its
        // choice actually launches (mirrors write_reaches_an_interactive_shell_and_its_echo_streams_back).
        let (program, args) = resolve_shell(Some(default_shell().0));
        let mut session = PtySession::spawn(&PtyLaunch {
            program,
            args,
            cwd: None,
            env: BTreeMap::new(),
            rows: 24,
            cols: 80,
        })
        .expect("spawn picked shell");
        let mut writer = session.writer().expect("writer");
        let line = if cfg!(windows) { "echo picked-shell-works\r\n" } else { "echo picked-shell-works\n" };
        writer.write_all(line.as_bytes()).expect("write");
        writer.flush().expect("flush");
        let out = drain_until(&session, "picked-shell-works", Duration::from_secs(10));
        assert!(out.contains("picked-shell-works"), "got: {out:?}");
        let _ = session.kill();
    }
}
