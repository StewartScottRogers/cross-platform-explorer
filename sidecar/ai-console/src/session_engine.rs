//! Session engine seam (CPE-309, slice S3): abstracts **where a session's PTY lives** so the console
//! can own it in-process (dev/tests) OR route it to the long-lived session-daemon process (so a
//! session survives a console restart — the reattach goal).
//!
//! `console.rs` no longer spawns `PtySession` directly; it asks a [`SessionEngine`] to `launch`, gets
//! back a [`SessionIo`], and reads its single output channel (raw PTY bytes; the sender drops on
//! agent exit). The ring/live-fanout/read-tap/usage/history logic in the console reader thread is
//! unchanged — it just consumes this channel instead of a raw reader, so both engines share one code
//! path. Input/resize/kill go through the `SessionIo`.
//!
//! - [`LocalEngine`] — PTY in this process (the historical behaviour; the default for tests).
//! - [`DaemonEngine`] — PTY in the session daemon; `launch`/`attach` proxy over a `SessionClient`, so
//!   the daemon (a separate process) keeps every session alive across a console restart, and
//!   [`SessionEngine::reattachable`] lists them for boot-time reattach (CPE-461 spanning a full
//!   process restart).

use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::pty::{PtyLaunch, PtySession};
use crate::session_client::{SessionClient, StreamMsg};
use crate::session_supervisor::SessionDaemonHandle;

/// Drives one session's I/O, regardless of where its PTY actually lives.
pub trait SessionIo: Send + Sync {
    /// Take the output stream (once). Chunks are raw PTY bytes; the sender is dropped when the agent
    /// exits (EOF), so the consumer's `recv` loop ends and the console runs its end-of-session path.
    fn take_output(&self) -> Option<Receiver<Vec<u8>>>;
    /// Send input bytes to the session's PTY.
    fn write(&self, bytes: &[u8]) -> Result<(), String>;
    /// Resize the session's terminal.
    fn resize(&self, rows: u16, cols: u16) -> Result<(), String>;
    /// Terminate the session (reclaims the child + PTY).
    fn kill(&self) -> Result<(), String>;
}

/// Creates + reattaches sessions. The console holds one `Arc<dyn SessionEngine>`.
pub trait SessionEngine: Send + Sync {
    /// Launch a new session `id`; returns its I/O handle.
    fn launch(&self, id: &str, launch: &PtyLaunch) -> Result<Arc<dyn SessionIo>, String>;
    /// Ids of sessions still alive in the engine's backing store — for reattach on console boot. A
    /// `LocalEngine` holds nothing across a restart (its PTYs died with the old process), so `[]`.
    fn reattachable(&self) -> Vec<String> {
        Vec::new()
    }
    /// Re-open the output stream of an already-running session (reattach). Returns `None` when the
    /// engine can't reattach (Local) or the session is unknown.
    fn attach(&self, _id: &str) -> Option<Arc<dyn SessionIo>> {
        None
    }
}

// ---------------------------------------------------------------------------------------------------
// Local (in-process) engine — the historical behaviour; default for dev + unit tests.
// ---------------------------------------------------------------------------------------------------

/// Owns PTYs in this process. No cross-restart survival (the whole point of the daemon engine).
#[derive(Default)]
pub struct LocalEngine;

struct LocalIo {
    pty: Mutex<PtySession>,
    writer: Mutex<Box<dyn Write + Send>>,
    out: Mutex<Option<Receiver<Vec<u8>>>>,
}

impl SessionEngine for LocalEngine {
    fn launch(&self, _id: &str, launch: &PtyLaunch) -> Result<Arc<dyn SessionIo>, String> {
        let session = PtySession::spawn(launch)?;
        let reader = session.reader()?;
        let writer = session.writer()?;
        // Pump the blocking reader into a channel so the console consumes both engines uniformly.
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break, // EOF / error → drop tx → consumer sees end
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break; // console stopped consuming
                        }
                    }
                }
            }
        });
        Ok(Arc::new(LocalIo {
            pty: Mutex::new(session),
            writer: Mutex::new(writer),
            out: Mutex::new(Some(rx)),
        }))
    }
}

impl SessionIo for LocalIo {
    fn take_output(&self) -> Option<Receiver<Vec<u8>>> {
        self.out.lock().unwrap().take()
    }
    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        let mut w = self.writer.lock().map_err(|_| "writer poisoned".to_string())?;
        w.write_all(bytes).map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())
    }
    fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.pty.lock().map_err(|_| "pty poisoned".to_string())?.resize(rows, cols)
    }
    fn kill(&self) -> Result<(), String> {
        self.pty.lock().map_err(|_| "pty poisoned".to_string())?.kill()
    }
}

// ---------------------------------------------------------------------------------------------------
// Daemon engine — PTYs live in the long-lived session-daemon process (survives a console restart).
// ---------------------------------------------------------------------------------------------------

/// Routes sessions to the session daemon via `SessionClient`s. Owns the [`SessionDaemonHandle`]; each
/// session gets its own client connection (its own control channel, so acks never cross sessions).
pub struct DaemonEngine {
    handle: SessionDaemonHandle,
}

impl DaemonEngine {
    pub fn new(handle: SessionDaemonHandle) -> DaemonEngine {
        DaemonEngine { handle }
    }

    /// Convert a `SessionClient` attach stream into the raw-bytes channel the console expects: replay
    /// + live output become byte chunks; `Exit` drops the sender (EOF).
    fn pump(stream: Receiver<StreamMsg>) -> Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            while let Ok(msg) = stream.recv() {
                match msg {
                    StreamMsg::Replay(b) | StreamMsg::Output(b) => {
                        if tx.send(b).is_err() {
                            break;
                        }
                    }
                    StreamMsg::Exit => break, // drop tx → EOF to the console
                }
            }
        });
        rx
    }

    fn io_for(&self, client: SessionClient, id: &str, stream: Receiver<StreamMsg>) -> Arc<dyn SessionIo> {
        Arc::new(DaemonIo {
            client: Arc::new(client),
            id: id.to_string(),
            out: Mutex::new(Some(Self::pump(stream))),
        })
    }
}

struct DaemonIo {
    client: Arc<SessionClient>,
    id: String,
    out: Mutex<Option<Receiver<Vec<u8>>>>,
}

impl SessionEngine for DaemonEngine {
    fn launch(&self, id: &str, launch: &PtyLaunch) -> Result<Arc<dyn SessionIo>, String> {
        let client = self.handle.client().map_err(|e| format!("connect daemon: {e}"))?;
        client.launch(id, launch)?;
        let stream = client.attach(id)?;
        Ok(self.io_for(client, id, stream))
    }

    fn reattachable(&self) -> Vec<String> {
        match self.handle.client() {
            Ok(c) => c.list().unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn attach(&self, id: &str) -> Option<Arc<dyn SessionIo>> {
        let client = self.handle.client().ok()?;
        let stream = client.attach(id).ok()?;
        Some(self.io_for(client, id, stream))
    }
}

impl SessionIo for DaemonIo {
    fn take_output(&self) -> Option<Receiver<Vec<u8>>> {
        self.out.lock().unwrap().take()
    }
    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        self.client.input(&self.id, bytes)
    }
    fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.client.resize(&self.id, rows, cols)
    }
    fn kill(&self) -> Result<(), String> {
        self.client.kill(&self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};

    fn echo_launch() -> PtyLaunch {
        let (program, args) = crate::pty::shell_command("echo engine-seam-works");
        PtyLaunch { program, args, cwd: None, env: BTreeMap::new(), rows: 24, cols: 80 }
    }

    fn drain(rx: &Receiver<Vec<u8>>, marker: &str, timeout: Duration) -> String {
        let mut seen = String::new();
        let deadline = Instant::now() + timeout;
        while !seen.contains(marker) {
            let Some(rem) = deadline.checked_duration_since(Instant::now()) else { break };
            match rx.recv_timeout(rem.min(Duration::from_millis(200))) {
                Ok(b) => seen.push_str(&String::from_utf8_lossy(&b)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        seen
    }

    #[test]
    fn local_engine_launches_and_streams_output() {
        let engine = LocalEngine;
        let io = engine.launch("s1", &echo_launch()).unwrap();
        let rx = io.take_output().expect("output taken once");
        assert!(drain(&rx, "engine-seam-works", Duration::from_secs(10)).contains("engine-seam-works"));
        assert!(io.take_output().is_none(), "output stream is take-once");
        let _ = io.kill();
    }

    #[test]
    fn local_engine_has_nothing_to_reattach() {
        assert!(LocalEngine.reattachable().is_empty());
        assert!(LocalEngine.attach("whatever").is_none());
    }
}
