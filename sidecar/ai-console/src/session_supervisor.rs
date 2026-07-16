//! Session-daemon supervisor (CPE-309) — the console-side building block that spawns and owns the
//! long-lived `--session-daemon` process, so agent PTYs live in a process that survives a restart of
//! this console UI process (see `docs/design/CPE-309-session-reattach.md`).
//!
//! This is the first concrete slice of the daemon integration: spawn the child, learn its port, hand
//! out `SessionClient`s, and reap the child on drop. Routing `ConsoleState`'s session ops through the
//! client (so a real restart reattaches) is the remaining step.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::session_client::SessionClient;

/// Owns the spawned session-daemon child process. Dropping it kills + reaps the child.
pub struct SessionDaemonHandle {
    child: Child,
    port: u16,
}

impl SessionDaemonHandle {
    /// Spawn `<exe> --session-daemon`, read the `PORT <n>` it announces on stdout, and return a
    /// handle. `exe` is the console's own executable (it knows the `--session-daemon` mode).
    pub fn spawn(exe: &Path) -> Result<SessionDaemonHandle, String> {
        let mut child = Command::new(exe)
            .arg("--session-daemon")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn session daemon: {e}"))?;
        let stdout = child.stdout.take().ok_or("session daemon: no stdout")?;
        let mut line = String::new();
        BufReader::new(stdout)
            .read_line(&mut line)
            .map_err(|e| format!("session daemon: read PORT: {e}"))?;
        let port = line
            .trim()
            .strip_prefix("PORT ")
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or_else(|| format!("session daemon: expected 'PORT <n>', got {line:?}"))?;
        Ok(SessionDaemonHandle { child, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// Connect a fresh client to the daemon (each pane/attach gets its own).
    pub fn client(&self) -> std::io::Result<SessionClient> {
        SessionClient::connect(&self.addr())
    }

    /// Whether the daemon child is still alive.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for SessionDaemonHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
