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
use crate::session_diag;

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
    ///
    /// **Not wired up yet, and that is load-bearing for CPE-1975.** This is the only reader and the
    /// only writer of the port file, and it has zero callers — in this crate, in `sidecar/host`, in
    /// `src-tauri`, and in every test. Production reaches the daemon a different way entirely: the
    /// host spawns it as its own child, reads `PORT <n>` off that child's stdout pipe, and passes the
    /// address in `CPE_AICONSOLE_SESSION_DAEMON_ADDR`, which `main.rs` feeds to
    /// [`SessionDaemonHandle::external`]. That is why the ticket's headline consequence — the console
    /// connecting to an attacker's daemon — is not reachable today.
    ///
    /// **When it is wired, hardening the port file's directory will not be enough.** `daemon_answers`
    /// below is the only thing standing between a port number and a `SessionClient`, and all it
    /// checks is that *something* replies to `{"op":"list"}` on loopback. It authenticates nothing.
    /// A shared token, minted by the daemon and printed beside `PORT <n>`, is the shape that would;
    /// see `console_temp_dir`'s module header for why it was not written here.
    pub fn discover_or_spawn(exe: &Path, port_file: &Path) -> Result<SessionDaemonHandle, String> {
        if let Some(port) = read_port_file(port_file) {
            if daemon_answers(port) {
                return Ok(SessionDaemonHandle { child: None, port });
            }
        }
        let handle = Self::spawn_detached(exe)?;
        // CPE-1975 round 2. This was `let _ = write_port_file(…)`, and round 1's claim that the
        // error is "propagated rather than dropped" was **false of the code**: making
        // `write_port_file` return the error only moved the swallow one frame up, to here.
        //
        // It is deliberately not `?`, and round 2's reason for that was **wrong**. It said `Err`
        // "would abandon a live daemon nobody holds a handle to — an orphan process". It would not:
        // `spawn_detached` returns `SessionDaemonHandle { child: Some(child), .. }`, and this type's
        // `Drop` reaps exactly that case (`child.kill()`, `child.wait()`). With `?` the handle drops
        // at the `?` and the daemon is **killed**.
        //
        // Which makes the conclusion stronger, not weaker: `?` here would hand the attacker a **kill
        // switch**. Plant a link at the rendezvous path → `write_port_file` refuses → `?` → `Drop`
        // reaps the daemon we just spawned → the console has no session daemon at all. A refusal must
        // never be able to take down the thing it is protecting, so the handle is returned and the
        // failure is reported instead.
        //
        // And it must genuinely be *reported*: after the hardening the only ways this fails are a
        // real I/O error and **a refusal**, i.e. something has planted a link or a non-regular file at
        // the rendezvous path — a security-relevant event, silent if the error is dropped.
        //
        // Round 2 reported it through `session_diag::trace` and claimed that "echoes to stderr
        // unconditionally". **False, and functionally so.** The `eprintln!` inside `trace` is
        // unconditional only *relative to the file writes below it*; `trace` itself opens with
        // `if !enabled() { return; }`, and `enabled()` requires one of four env vars. On the very
        // path this code exists for, none of them is set: `CPE_AICONSOLE_DIAG` is set only by
        // `run_session_daemon()`, in the **daemon** process ("process-local; it does not affect the
        // UI sidecar"), while `discover_or_spawn` runs in the **console** process; and
        // `CPE_AICONSOLE_SESSION_DAEMON_ADDR` is set only on the host-injected path, which uses
        // `SessionDaemonHandle::external` *instead of* this function. So the report went to nobody by
        // default — the exact outcome the change was made to prevent. The lesson, worth carrying: a
        // report is only as good as the channel it lands in. **Check the channel, not just the call.**
        //
        // So the refusal goes to the real stderr handle **directly**, with no gate in front of it —
        // the same pattern, and for the same reason, as the `writeln!(std::io::stderr(), …)` notices
        // in `tests/console_temp_dir_containment.rs`. The `trace` call is kept as well, so the line
        // also reaches the diagnostic log when tracing happens to be on.
        if let Err(e) = write_port_file(port_file, handle.port) {
            let msg = format!(
                "could not record the daemon port at {}: {e} — the daemon is running on 127.0.0.1:{} \
                 but a restarted console will not rediscover it. After CPE-1975 the only causes are a \
                 real I/O error or a REFUSAL, i.e. something is planted at that path; look at it \
                 before removing it.",
                port_file.display(),
                handle.port
            );
            // Guarded by `src/lib/consoleRefusalReport.test.ts`, which derives this region's shape
            // from this source and reds if the ungated `writeln!` goes away. It strips comments
            // first, and that is load-bearing rather than ceremonial — **measured, not asserted**:
            // its scanned region starts at the `spawn_detached` line above, so the comment block
            // between there and here (which quotes both `writeln!(std::io::stderr(), …)` and
            // `enabled()`) falls inside it. With the strip off and this line deleted, the guard goes
            // GREEN on the comment's quotation. The four-cell table is in that file's header.
            // (Round 3's attempt to pin this from `session_diag`'s unit module could not fire at
            // all — deleting this line left the crate green at 423/0.)
            let _ = writeln!(std::io::stderr(), "[CPE-1975] {msg}");
            session_diag::trace("supervisor", &msg);
        }
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
///
/// **This is a liveness probe, not an authentication check (CPE-1975).** Any loopback listener that
/// writes one byte back passes it. It is safe today only because the port it is handed comes from a
/// caller with no callers; see [`SessionDaemonHandle::discover_or_spawn`].
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

/// Read the daemon's port out of the port file, refusing a redirected path (CPE-1975).
///
/// `read_to_string` follows every link on the way, so without the two checks a junction planted at
/// `<temp>/cpe-ai-console` — or a symlink at the port file's own name inside it — lets an attacker
/// choose the port this console then connects to. See `console_temp_dir`'s header for why that is
/// currently unreachable (nothing calls [`SessionDaemonHandle::discover_or_spawn`]) and why the
/// check is here anyway.
///
/// **A read path must not create anything.** Round 1 called `ensure_console_dir_at` here, which
/// `mkdir`s the rendezvous directory as a side effect of a lookup — harmless (a refusal still
/// returns `None`, so the caller spawns a fresh daemon, which is the right fail-safe) but surprising,
/// and a reader that writes is the kind of thing the next person has to re-derive. It now uses
/// [`console_temp_dir::console_dir_is_real`], which only `lstat`s. A directory that is not there yet
/// answers "no port file", which is exactly right on a first run.
fn read_port_file(path: &Path) -> Option<u16> {
    let dir = path.parent()?;
    if !crate::console_temp_dir::console_dir_is_real(dir) {
        return None;
    }
    if !crate::console_temp_dir::regular_file_or_absent(path) {
        return None;
    }
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Record the daemon's port, refusing a redirected path (CPE-1975).
///
/// Was `let _ = std::fs::create_dir_all(dir);` followed by an unconditional write — the second of the
/// two `create_dir_all` sites the ticket names. The refusal now **fails this function** instead of
/// being swallowed inside it, so a port file that could not be written safely does not look like one
/// that was.
///
/// The signature was already `io::Result<()>` before CPE-1975, so returning the error is only half a
/// change: round 1 claimed it was "propagated rather than dropped", which was **not true of the
/// code** — the sole caller had `let _ = write_port_file(…)`, so the swallow simply moved one frame
/// up. [`SessionDaemonHandle::discover_or_spawn`] now handles it, and says at that site why it
/// reports rather than returns.
fn write_port_file(path: &Path, port: u16) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "port file path has no parent directory")
    })?;
    crate::console_temp_dir::ensure_console_dir_at(dir)?;
    if !crate::console_temp_dir::regular_file_or_absent(path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} is not a plain file, so the daemon port will not be written through it (CPE-1975)", path.display()),
        ));
    }
    std::fs::write(path, port.to_string())
}

/// The standard place to record the running daemon's port, so a restarted console rediscovers it.
///
/// Both path components come from `console_temp_dir` (CPE-1975); they used to be spelled inline here,
/// which is how three sites ended up with three hand-written copies of one path.
pub fn default_port_file() -> PathBuf {
    crate::console_temp_dir::console_temp_dir().join(crate::console_temp_dir::PORT_FILE_NAME)
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
