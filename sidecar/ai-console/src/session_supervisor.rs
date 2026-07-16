//! Session-daemon supervisor (CPE-309) — the console-side building block that spawns and owns the
//! long-lived `--session-daemon` process, so agent PTYs live in a process that survives a restart of
//! this console UI process (see `docs/design/CPE-309-session-reattach.md`).
//!
//! This is the first concrete slice of the daemon integration: spawn the child, learn its port, hand
//! out `SessionClient`s, and reap the child on drop. Routing `ConsoleState`'s session ops through the
//! client (so a real restart reattaches) is the remaining step.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use crate::session_client::SessionClient;

/// Owns (or references) the session-daemon process. A daemon we **spawned** is reaped on drop; a
/// daemon we **discovered** already running (across a console restart) is left alive — it must
/// outlive this console process, which is the whole point (CPE-309 S4).
pub struct SessionDaemonHandle {
    /// `Some` only when we spawned it; `None` when we reconnected to an already-running daemon.
    child: Option<Child>,
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
        Ok(SessionDaemonHandle { child: Some(child), port })
    }

    /// Reconnect to a daemon **already running** at `port` (across a console restart), or spawn a new
    /// one if none answers there, recording the live port in `port_file` for the next restart to find
    /// (CPE-309 S4). The daemon is spawned **detached** so it survives this console process exiting;
    /// on a discovered daemon we do NOT own the child, so dropping this handle leaves it running.
    pub fn discover_or_spawn(exe: &Path, port_file: &Path) -> Result<SessionDaemonHandle, String> {
        if let Some(port) = read_port_file(port_file) {
            if daemon_answers(port) {
                return Ok(SessionDaemonHandle { child: None, port });
            }
        }
        let handle = Self::spawn_detached(exe)?;
        let _ = write_port_file(port_file, handle.port);
        Ok(handle)
    }

    /// Spawn `<exe> --session-daemon` detached from this process group/job so it can outlive the
    /// console (CPE-309 S4). On Windows we request `CREATE_BREAKAWAY_FROM_JOB` best-effort — whether
    /// it takes depends on the host's job-object policy; the runtime restart check confirms it.
    pub fn spawn_detached(exe: &Path) -> Result<SessionDaemonHandle, String> {
        let mut cmd = Command::new(exe);
        cmd.arg("--session-daemon").stdout(Stdio::piped()).stderr(Stdio::null());
        detach(&mut cmd);
        let mut child = cmd.spawn().map_err(|e| format!("spawn session daemon: {e}"))?;
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
        Ok(SessionDaemonHandle { child: Some(child), port })
    }

    /// Reference a daemon the **host** already spawned + owns (CPE-309 S4), at `port`. We never reap
    /// it (the host does, on app exit), so it survives this UI sidecar restarting. This is the
    /// production path: the host spawns the daemon with a hidden console (ConPTY works) and outside
    /// the UI sidecar's lifetime (it survives), then passes the port here.
    pub fn external(port: u16) -> SessionDaemonHandle {
        SessionDaemonHandle { child: None, port }
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

    /// Whether a daemon child we spawned is still alive (a discovered daemon reports `true` — it is
    /// running, we just don't own the process).
    pub fn is_running(&mut self) -> bool {
        match &mut self.child {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => daemon_answers(self.port),
        }
    }
}

impl Drop for SessionDaemonHandle {
    fn drop(&mut self) {
        // Only reap a daemon we spawned AND still intend to own. A discovered daemon (child: None)
        // is deliberately left running so it survives this console (CPE-309 S4).
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A daemon answers if a `list` over its socket returns promptly.
fn daemon_answers(port: u16) -> bool {
    let Ok(mut sock) = TcpStream::connect(("127.0.0.1", port)) else { return false };
    let _ = sock.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = sock.set_write_timeout(Some(Duration::from_millis(500)));
    if sock.write_all(b"{\"op\":\"list\"}\n").is_err() {
        return false;
    }
    let mut buf = [0u8; 1];
    matches!(std::io::Read::read(&mut sock, &mut buf), Ok(n) if n > 0)
}

fn read_port_file(path: &Path) -> Option<u16> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn write_port_file(path: &Path, port: u16) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, port.to_string())
}

/// The standard place to record the running daemon's port, so a restarted console rediscovers it.
pub fn default_port_file() -> PathBuf {
    std::env::temp_dir().join("cpe-ai-console").join("session-daemon.port")
}

/// Detach the spawned daemon from the parent's process group/job so it outlives the console.
#[cfg(windows)]
fn detach(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB (best-effort).
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
}

#[cfg(unix)]
fn detach(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // New session (setsid) so it isn't in the console's process group / controlling terminal.
    unsafe {
        cmd.pre_exec(|| {
            libc_setsid();
            Ok(())
        });
    }
}

#[cfg(unix)]
fn libc_setsid() {
    // Avoid a libc dep: call setsid via the raw syscall is overkill; use the process API instead.
    // SAFETY: setsid() has no memory effects; ignoring the result is fine (fails only if already a
    // group leader, which a freshly-forked child is not).
    extern "C" {
        fn setsid() -> i32;
    }
    unsafe {
        let _ = setsid();
    }
}
