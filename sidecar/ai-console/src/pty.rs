//! Embedded PTY console (CPE-280).
//!
//! Runs a native CLI inside a real pseudo-terminal so interactive agent TUIs
//! (`claude`, `aider`, `codex`, …) work in-app — full streamed output, keyboard input,
//! resize, and clean shutdown. Cross-platform via `portable-pty` (Windows ConPTY, Unix
//! openpty). The session's working directory is set to the repo the explorer has open
//! (via the context capability), and its environment comes from the routing engine +
//! credential vault — secrets are injected into the child, never logged.

use std::collections::BTreeMap;
use std::io::{Read, Write};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

/// A running agent session in a pseudo-terminal.
pub struct PtySession {
    master: Box<dyn portable_pty::MasterPty + Send>,
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

        Ok(PtySession { master: pair.master, child })
    }

    /// A reader for the terminal's output stream (clone of the master).
    pub fn reader(&self) -> Result<Box<dyn Read + Send>, String> {
        self.master.try_clone_reader().map_err(|e| e.to_string())
    }

    /// A writer for the terminal's input stream.
    pub fn writer(&self) -> Result<Box<dyn Write + Send>, String> {
        self.master.take_writer().map_err(|e| e.to_string())
    }

    /// Resize the terminal (on window/pane resize).
    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.master
            .resize(PtySize { rows: rows.max(1), cols: cols.max(1), pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())
    }

    /// True while the child is still running.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Terminate the session.
    pub fn kill(&mut self) -> Result<(), String> {
        self.child.kill().map_err(|e| e.to_string())
    }

    /// Wait for the child to exit and return its exit code.
    pub fn wait(&mut self) -> Result<u32, String> {
        self.child.wait().map(|s| s.exit_code()).map_err(|e| e.to_string())
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
}
