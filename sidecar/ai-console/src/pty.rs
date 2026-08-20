//! Embedded PTY console (CPE-280).
//!
//! Runs a native CLI inside a real pseudo-terminal so interactive agent TUIs
//! (`claude`, `aider`, `codex`, …) work in-app — full streamed output, keyboard input,
//! resize, and clean shutdown. Cross-platform via `portable-pty` (Windows ConPTY, Unix
//! openpty). The session's working directory is set to the repo the explorer has open
//! (via the context capability), and its environment comes from the routing engine +
//! credential vault — secrets are injected into the child, never logged.
//!
//! Lifecycle hardening (CPE-1246, porting the main app's CPE-1244 fix back to this — the
//! **original** pattern CPE-1244 mirrored): [`PtySession::kill`] does a final `wait()` after the kill
//! so a SIGKILL-escalated child is always reaped (no zombie), and [`PtySession`] has a belt-and-braces
//! [`Drop`] impl that kills+reaps too. The shutdown-sweep half of CPE-1244 (a `close_all` sweeping
//! every live session, wired to the process's own exit path) was **already present here** before this
//! ticket — `ConsoleState::close_all` (`console.rs`) and `SessionDaemon::close_all`
//! (`session_daemon.rs`) both existed from CPE-442, and both are already wired to this process's exit
//! (`main.rs`'s `Reaction::Exit` handler, and the `close_all` op in `session_server.rs`); every session
//! kill in this crate — local or daemon-hosted — bottoms out in [`PtySession::kill`], so this file's
//! fix alone closes the reap gap everywhere.

use std::collections::BTreeMap;
use std::io::{Read, Write};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

/// A running agent session in a pseudo-terminal.
pub struct PtySession {
    /// `Some` while the session is live. `kill` drops it, which closes the ConPTY so the output reader
    /// EOFs — the only way a Windows ConPTY reader ends when the session is *held* by the console rather
    /// than owned+dropped by the caller (CPE-574; on Unix the reader EOFs on child exit already).
    master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

/// How to launch a session.
pub struct PtyLaunch {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    /// Extra environment (from routing + vault). Injected into the child only.
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

        // On Windows, run through `cmd /c` so script shims (npm/pip CLIs) resolve (CPE-326).
        let (program, args) = crate::cli_command(&launch.program, &launch.args);
        let mut cmd = CommandBuilder::new(&program);
        cmd.args(&args);
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

    /// A writer for the terminal's input stream.
    pub fn writer(&self) -> Result<Box<dyn Write + Send>, String> {
        self.master()?.take_writer().map_err(|e| e.to_string())
    }

    /// Resize the terminal (on window/pane resize).
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.master()?
            .resize(PtySize { rows: rows.max(1), cols: cols.max(1), pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())
    }

    /// True while the child is still running.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Non-blocking exit check: `Some(code)` if the child has exited, `None` if it's still running.
    /// Polls the process handle, so it detects exit even on Windows ConPTY where the output pipe stays
    /// open until the master closes (used by the live swarm driver, CPE-541).
    pub fn try_wait(&mut self) -> Option<u32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.exit_code()),
            _ => None,
        }
    }

    /// Terminate the session: kill the child and **close the ConPTY** (drop the master). Closing the
    /// master is what EOFs the output reader on Windows ConPTY when the session is held by the console
    /// (not owned+dropped by the caller) — without it an adopted swarm session never reports completion
    /// (CPE-574). Idempotent.
    ///
    /// CPE-1246 (porting the main app's CPE-1244 fix to this, the original pattern it was mirrored
    /// from): portable-pty 0.8.1's `ChildKiller` (the unix `impl ChildKiller for std::process::Child` in
    /// its `lib.rs`) sends SIGHUP, polls `try_wait` ~5×50ms for a graceful exit, then — if the child
    /// ignored SIGHUP — escalates to a bare `std::process::Child::kill()` (SIGKILL) with **no follow-up
    /// wait**. `Child::kill()` only *sends* the signal; nothing reaps the now-dead child, so a
    /// SIGHUP-ignoring child would be SIGKILL'd but left a transient zombie until something else waits on
    /// it. The explicit `self.child.wait()` below closes that gap: it's a no-op/instant when the
    /// grace-period polling already reaped the child (`std::process::Child` caches the exit status once
    /// observed), and it's the actual reap when escalation was needed.
    pub fn kill(&mut self) -> Result<(), String> {
        // CPE-1268: an idempotent kill must tolerate an already-exited/already-reaped child. On Unix,
        // once a prior `try_wait()`/`is_running()` poll (or `kill()` itself) has reaped the process,
        // `std::process::Child::kill()` returns `Err` ("invalid argument: can't kill an exited
        // process" / ESRCH). That is NOT a failure — the child is already dead, which is exactly what
        // kill() promises. Fast-path it: if the child has already exited, closing the ConPTY is all
        // that's left to do.
        if self.try_wait().is_some() {
            self.master = None; // drop the master → close the ConPTY → the output reader EOFs
            return Ok(());
        }
        let kill_res = self.child.kill();
        let _ = self.child.wait(); // reap unconditionally -- see CPE-1246/CPE-1244 doc comment above
        self.master = None; // drop the master → close the ConPTY → the output reader EOFs
        match kill_res {
            Ok(()) => Ok(()),
            // The child may have exited on its own in the window between the try_wait check above and
            // the kill signal; the unconditional wait() above has since reaped it, so a kill error that
            // only means "the process was already gone" is success, not failure.
            Err(_) if self.try_wait().is_some() => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Wait for the child to exit and return its exit code.
    pub fn wait(&mut self) -> Result<u32, String> {
        self.child.wait().map(|s| s.exit_code()).map_err(|e| e.to_string())
    }
}

impl Drop for PtySession {
    /// Belt-and-braces (CPE-1246, porting CPE-1244): every current caller already calls `kill()` before
    /// a session is dropped (`ConsoleState::close_session`/`close_all`, `SessionDaemon::kill`/`close_all`,
    /// and the tests below), so in practice this is a no-op. But it costs nothing to make dropping a live
    /// session *always* kill + reap the child too, so a future code path that forgets the explicit
    /// `kill()` can never leak a running agent process or a zombie. Idempotent/cheap on an
    /// already-killed session: `child.kill()` on a dead process errors (discarded) and `child.wait()`
    /// returns the cached exit status instantly.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The launch command + args for an OS shell running a one-off command — used to
/// exercise the PTY cross-platform in tests and as a sane default console shell.
pub fn shell_command(inline: &str) -> (String, Vec<String>) {
    if cfg!(target_os = "windows") {
        ("cmd".to_string(), vec!["/c".to_string(), inline.to_string()])
    } else {
        ("sh".to_string(), vec!["-c".to_string(), inline.to_string()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::{Duration, Instant};

    /// Drain the session's output on a background thread until `marker` appears or the
    /// deadline passes, returning what was read. Timed because a Windows ConPTY master
    /// may never signal EOF, so a plain `read_to_string` would block forever.
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

    fn spawn(inline: &str, env: BTreeMap<String, String>) -> PtySession {
        let (program, args) = shell_command(inline);
        PtySession::spawn(&PtyLaunch { program, args, cwd: None, env, rows: 24, cols: 80 })
            .expect("spawn pty")
    }

    #[test]
    fn runs_a_command_in_a_real_pty_and_streams_output() {
        let mut session = spawn("echo pty-works", BTreeMap::new());
        let out = drain_until(&session, "pty-works", Duration::from_secs(10));
        assert!(out.contains("pty-works"), "got: {out:?}");
        let _ = session.kill();
    }

    #[test]
    fn env_is_injected_into_the_child() {
        let inline = if cfg!(target_os = "windows") { "echo %CPE_TEST_VAR%" } else { "echo $CPE_TEST_VAR" };
        let mut env = BTreeMap::new();
        env.insert("CPE_TEST_VAR".to_string(), "injected-value".to_string());
        let mut session = spawn(inline, env);
        let out = drain_until(&session, "injected-value", Duration::from_secs(10));
        assert!(out.contains("injected-value"), "got: {out:?}");
        let _ = session.kill();
    }

    #[test]
    fn resize_and_liveness_then_kill() {
        // A command that stays alive briefly so we can resize + observe liveness.
        let inline = if cfg!(target_os = "windows") { "ping -n 4 127.0.0.1 >NUL" } else { "sleep 2" };
        let mut session = spawn(inline, BTreeMap::new());
        assert!(session.resize(40, 120).is_ok());
        assert!(session.is_running());
        session.kill().unwrap();
    }

    /// OS-level "is this pid still around" check (CPE-1246, ported from CPE-1244's manual UAT
    /// `tasklist` check): `tasklist` on Windows, `ps -p` elsewhere (works on both Linux and macOS CI
    /// runners, unlike a Linux-only `/proc/<pid>` check). Used to prove `kill()`/`Drop` actually
    /// terminate the OS process, not just close our handle to it.
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

    /// CPE-1818: assert `pid` is gone, but **bounded-poll** rather than checking once and panicking
    /// immediately. `pid_is_alive` (above) asks the OS's own process table (`tasklist`/`ps`) whether the
    /// PID is still enumerable, and releasing our last handle to a child does not guarantee that table
    /// is scrubbed synchronously -- on Windows in particular, `CloseHandle` schedules the process
    /// object's deletion but doesn't have to complete it before the call returns, so `tasklist` can
    /// still see the PID for a while after the handle is gone, worse under a loaded runner.
    ///
    /// The 10s deadline (matching this file's existing convention for output-drain polling elsewhere in
    /// this module) is a **generous ceiling for a scheduling gap, not a tuned estimate** of how long
    /// teardown "should" take. It costs nothing on the happy path -- the loop returns the instant
    /// `pid_is_alive` goes false, which every idle run does in well under a second -- and it does not
    /// weaken the guarantee this check pins: a PID that is genuinely still alive (a real leak) never
    /// satisfies `!pid_is_alive` no matter how long you wait, so this still reds at the deadline,
    /// deterministically, exactly like the single-shot assert did. It just no longer mistakes "not
    /// reaped *yet*" for "not reaped at all". (CPE-1818 review round 1 measured a 2s deadline failing
    /// 1/10 under heavy combined contention -- a parallel build *and* a second competing `cargo test`
    /// loop on a 32-core box -- at 6.59s wall; 10s leaves headroom past that, and `windows-latest` CI
    /// runners are weaker and more oversubscribed than any dev box, so a shorter number would still be
    /// a guess dressed up as a measurement.)
    ///
    /// **Accepted residual: PID reuse.** `pid_is_alive` matches purely on the numeric PID (`tasklist
    /// /FI "PID eq {pid}" /NH` containment, or `ps -p` on Unix) with no process-name or start-time
    /// cross-check, so it cannot distinguish "our child is still alive" from "an unrelated process the
    /// OS handed the same PID after ours exited". The pre-CPE-1818 single-shot check had a near-zero
    /// exposure window to that race; polling widens it to the full deadline, and a 10s deadline widens
    /// it further than 2s did. This is accepted rather than closed: Windows does not rapidly recycle
    /// recently-freed PIDs (its allocator cycles through a large space before reusing one), so the odds
    /// of an unrelated process landing on exactly this PID inside the poll window are low, and this
    /// helper is test-only, never production. Closing it properly would mean cross-checking the child's
    /// process start time or image name (e.g. `CreateToolhelp32Snapshot`/`GetProcessTimes` on Windows,
    /// or parsing `tasklist`'s image-name column) against a value captured before teardown -- not done
    /// here, out of scope for a flake fix, but written down so it isn't a silent unknown. Ported from
    /// the app-side `src-tauri/src/pty.rs` fix for the same flake (CPE-1818's sibling sweep).
    fn assert_pid_reaped_within(pid: u32, timeout: Duration, msg: &str) {
        let deadline = Instant::now() + timeout;
        loop {
            if !pid_is_alive(pid) {
                return;
            }
            assert!(Instant::now() < deadline, "{msg}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn kill_reaps_the_child_synchronously_with_no_zombie_left_behind() {
        // CPE-1246 (porting CPE-1244): `kill()` now does its own final `wait()`, so the reap should be
        // observable *immediately* after `kill()` returns -- no polling loop needed. A long-lived
        // command so we're sure it's still running right before the kill.
        let inline = if cfg!(target_os = "windows") { "ping -n 10 127.0.0.1 >NUL" } else { "sleep 10" };
        let mut session = spawn(inline, BTreeMap::new());
        assert!(session.is_running(), "child should still be running before kill()");
        let pid = session.child.process_id().expect("child should report a pid");
        assert!(pid_is_alive(pid), "child process should be alive before kill()");

        session.kill().unwrap();

        // No sleep/poll: kill()'s own follow-up wait() must have already reaped it.
        assert!(session.try_wait().is_some(), "child was not reaped synchronously by kill()");
        // CPE-1818: bounded-poll -- `session` (and its held `child`) is still in scope here, and the OS
        // process-table teardown following `kill()`'s reap is asynchronous, same reasoning as
        // `assert_pid_reaped_within`'s doc comment.
        assert_pid_reaped_within(pid, Duration::from_secs(10), "OS process should be gone right after kill()");
    }

    #[test]
    fn dropping_a_live_session_without_an_explicit_kill_still_reaps_the_child() {
        // CPE-1246 (porting CPE-1244): the belt-and-braces `Drop` impl. Every real caller
        // (`ConsoleState::close_session`/`close_all`, `SessionDaemon::kill`/`close_all`) already calls
        // `kill()` explicitly, but a future path that forgets to must never leak a running process or
        // leave a zombie -- dropping the session alone has to be enough.
        let inline = if cfg!(target_os = "windows") { "ping -n 10 127.0.0.1 >NUL" } else { "sleep 10" };
        let mut session = spawn(inline, BTreeMap::new());
        assert!(session.is_running(), "child should still be running before drop");
        let pid = session.child.process_id().expect("child should report a pid");
        assert!(pid_is_alive(pid), "child process should be alive before drop");

        drop(session); // no explicit kill() -- Drop alone must terminate + reap the child

        // CPE-1818: bounded-poll, same reasoning as `assert_pid_reaped_within`'s doc comment.
        assert_pid_reaped_within(pid, Duration::from_secs(10), "OS process should be gone right after drop");
    }

    #[test]
    fn kill_on_an_already_exited_child_is_idempotent_and_reaps_cleanly() {
        // CPE-1246: `kill()`'s unconditional `wait()` must stay a cheap no-op when the grace-period
        // polling (or the child exiting on its own) already reaped the process -- not hang or error.
        let mut session = spawn("echo done-already", BTreeMap::new());
        let out = drain_until(&session, "done-already", Duration::from_secs(10));
        assert!(out.contains("done-already"), "got: {out:?}");
        // Give the child a moment to actually exit on its own before we kill an already-dead process.
        let deadline = Instant::now() + Duration::from_secs(10);
        while session.is_running() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!session.is_running(), "child should have exited on its own");
        assert!(session.kill().is_ok(), "kill() on an already-exited child should not error");
    }
}
