//! Session reattach engine (CPE-309).
//!
//! The AI Console UI process (the WebSocket server in [`crate::console`]) can be restarted
//! — a crash, an app update that swaps the sidecar binary, or the user toggling the mode
//! off and on. Today each agent PTY is owned directly by that process, so a restart kills
//! every running agent. This engine is the piece that makes a session **survive**: it owns
//! the PTYs and their output independently of any attached client, so a client can drop and
//! a *new* client can re-**attach** — receiving a replay of what it missed, then live output.
//!
//! This is the reattach *mechanism*, deliberately decoupled from transport: a [`SessionDaemon`]
//! holds sessions; `attach` hands back the buffered scrollback plus a live receiver. A future
//! slice runs one of these in a **separate, long-lived process** behind a loopback socket and
//! points the UI server at it (see the module's "Remaining wiring" note and CPE-309); the
//! engine here is what that process is built around, and it is what the tests exercise.
//!
//! Contrast with CPE-370 (history): that preserves a *finished* session's transcript so it can
//! be relaunched. This preserves a *running* session so its live I/O can be resumed.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use crate::pty::{PtyLaunch, PtySession};

/// Max bytes of output buffered per session for replay-on-reattach. A long-running agent
/// can print a lot; we keep a bounded tail so a reattaching client gets recent context
/// without the daemon growing without bound (mirrors the console ring, CPE-334).
pub const REPLAY_RING_CAP: usize = 256 * 1024;

/// A live output subscriber — the sending half of a client's stream.
type Subscriber = mpsc::Sender<Vec<u8>>;

/// One PTY-backed session owned by the daemon, independent of any client connection.
struct DaemonSession {
    pty: Mutex<PtySession>,
    /// Bounded tail of everything the PTY has emitted, for replay on (re)attach.
    ring: Arc<Mutex<Vec<u8>>>,
    /// Everyone currently attached; the reader thread fans each chunk out to all of them
    /// and prunes any whose receiver has been dropped (a disconnected/restarted client).
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
    /// Set once the PTY reaches EOF (the agent exited).
    exited: Arc<Mutex<bool>>,
}

/// Owns running sessions so they outlive the client(s) attached to them (CPE-309).
#[derive(Default)]
pub struct SessionDaemon {
    sessions: Mutex<HashMap<String, Arc<DaemonSession>>>,
}

/// The result of attaching to a session: what to replay, then the live stream.
pub struct Attachment {
    /// The buffered scrollback to write to the client first (what it missed).
    pub replay: Vec<u8>,
    /// Live output from now on. Dropping the receiver detaches — the session keeps running.
    pub live: mpsc::Receiver<Vec<u8>>,
}

impl SessionDaemon {
    pub fn new() -> SessionDaemon {
        SessionDaemon::default()
    }

    /// Launch `launch` as a new session under `id`. The session runs until the agent exits
    /// or [`kill`](Self::kill) is called — **not** tied to any client. Errors if `id` is
    /// already live or the PTY can't spawn.
    pub fn launch(&self, id: &str, launch: &PtyLaunch) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.contains_key(id) {
            return Err(format!("session '{id}' already exists"));
        }
        let session = PtySession::spawn(launch)?;
        let reader = session.reader()?;
        let ds = Arc::new(DaemonSession {
            pty: Mutex::new(session),
            ring: Arc::new(Mutex::new(Vec::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            exited: Arc::new(Mutex::new(false)),
        });
        Self::spawn_reader(Arc::clone(&ds), reader);
        sessions.insert(id.to_string(), ds);
        Ok(())
    }

    /// Pump the PTY's output into the ring and out to every subscriber until EOF. Runs for
    /// the life of the session, regardless of whether anyone is attached — that is exactly
    /// what lets output accumulate for a client that attaches *later*.
    fn spawn_reader(ds: Arc<DaemonSession>, mut reader: Box<dyn Read + Send>) {
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let chunk = &buf[..n];
                        {
                            let mut ring = ds.ring.lock().unwrap();
                            ring.extend_from_slice(chunk);
                            let overflow = ring.len().saturating_sub(REPLAY_RING_CAP);
                            if overflow > 0 {
                                ring.drain(..overflow);
                            }
                        }
                        // Fan out, dropping any subscriber whose receiver is gone.
                        let mut subs = ds.subscribers.lock().unwrap();
                        subs.retain(|tx| tx.send(chunk.to_vec()).is_ok());
                    }
                }
            }
            *ds.exited.lock().unwrap() = true;
            // Wake attached clients so their read loop can observe the exit and close.
            ds.subscribers.lock().unwrap().clear();
        });
    }

    /// Attach (or re-attach) a client to a running session: returns the buffered scrollback
    /// to replay, plus a live receiver for subsequent output. Multiple clients may attach at
    /// once. Errors if the session is unknown.
    pub fn attach(&self, id: &str) -> Result<Attachment, String> {
        let sessions = self.sessions.lock().unwrap();
        let ds = sessions.get(id).ok_or_else(|| format!("no such session '{id}'"))?;
        let replay = ds.ring.lock().unwrap().clone();
        let (tx, rx) = mpsc::channel();
        // Only register for live output if the session is still running; if it already
        // exited, the caller still gets the full replay and an immediately-closed stream.
        if !*ds.exited.lock().unwrap() {
            ds.subscribers.lock().unwrap().push(tx);
        }
        Ok(Attachment { replay, live: rx })
    }

    /// Write bytes to a session's PTY input (keystrokes from the attached client).
    pub fn input(&self, id: &str, bytes: &[u8]) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        let ds = sessions.get(id).ok_or_else(|| format!("no such session '{id}'"))?;
        let pty = ds.pty.lock().unwrap();
        let mut writer = pty.writer()?;
        writer.write_all(bytes).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())
    }

    /// Resize a session's terminal.
    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        let ds = sessions.get(id).ok_or_else(|| format!("no such session '{id}'"))?;
        let pty = ds.pty.lock().unwrap();
        pty.resize(rows, cols)
    }

    /// Whether a session exists and its agent is still running.
    pub fn is_running(&self, id: &str) -> bool {
        let sessions = self.sessions.lock().unwrap();
        match sessions.get(id) {
            Some(ds) => {
                if *ds.exited.lock().unwrap() {
                    return false;
                }
                let running = ds.pty.lock().unwrap().is_running();
                running
            }
            None => false,
        }
    }

    /// The ids of all sessions the daemon currently holds (running or exited-but-unreaped).
    pub fn list(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.sessions.lock().unwrap().keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Kill a session's agent and drop it from the daemon. Idempotent-ish: an unknown id errors.
    pub fn kill(&self, id: &str) -> Result<(), String> {
        let ds = self
            .sessions
            .lock()
            .unwrap()
            .remove(id)
            .ok_or_else(|| format!("no such session '{id}'"))?;
        let mut pty = ds.pty.lock().unwrap();
        pty.kill()
    }

    /// Kill and drop **every** session at once (CPE-442) — the daemon-side fan-out teardown, mirror
    /// of [`ConsoleState::close_all`](crate::console). Reclaims each agent child + PTY and empties the
    /// set, so a daemon process teardown leaves no orphaned agents. Idempotent: with nothing held it
    /// returns an empty list. Returns the closed ids, sorted.
    pub fn close_all(&self) -> Vec<String> {
        let drained: Vec<(String, Arc<DaemonSession>)> =
            self.sessions.lock().unwrap().drain().collect();
        let mut ids: Vec<String> = Vec::with_capacity(drained.len());
        for (id, ds) in drained {
            let _ = ds.pty.lock().unwrap().kill();
            ids.push(id);
        }
        ids.sort();
        ids
    }

    /// Drop any sessions whose agent has already exited, freeing their handles. Interaction
    /// with resource budgets (CPE-297): the supervisor samples memory of the daemon process;
    /// reaping exited sessions here keeps that footprint bounded, and a killed/oversized
    /// session is removed the same way. Returns the reaped ids.
    pub fn reap_exited(&self) -> Vec<String> {
        let mut sessions = self.sessions.lock().unwrap();
        // Base "exited" on the child's actual status via `try_wait`, not the reader-thread EOF
        // flag: a Windows ConPTY master may never signal EOF, so the flag can lag, but
        // `PtySession::is_running` (try_wait) reflects the real process state on every OS.
        let dead: Vec<String> = sessions
            .iter()
            .filter(|(_, ds)| !ds.pty.lock().unwrap().is_running())
            .map(|(id, _)| id.clone())
            .collect();
        for id in &dead {
            sessions.remove(id);
        }
        dead
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};

    /// Read from a live receiver until `marker` is seen or the deadline passes; returns what
    /// was read. Timed because a ConPTY master may never signal EOF (same reason as pty.rs).
    fn recv_until(rx: &mpsc::Receiver<Vec<u8>>, marker: &str, timeout: Duration) -> String {
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

    /// Wait for `marker` from an attachment, accepting it from the `replay` OR the live stream
    /// (CPE-415). On a loaded machine the marker can be emitted *before* the client attaches, so it
    /// arrives in the replay, not live — checking only `live` is a race. Accumulates both.
    fn drain_attach(att: &Attachment, marker: &str, timeout: Duration) -> String {
        let mut seen = String::from_utf8_lossy(&att.replay).into_owned();
        if !seen.contains(marker) {
            seen.push_str(&recv_until(&att.live, marker, timeout));
        }
        seen
    }

    /// A cross-platform command that prints READY, waits, prints TICK, then stays alive a while.
    /// The gap before TICK is generous so a reattaching client reliably connects before it fires.
    fn ready_then_tick() -> PtyLaunch {
        let (program, args) = if cfg!(target_os = "windows") {
            (
                "cmd".to_string(),
                vec![
                    "/c".to_string(),
                    "echo READY& ping -n 4 127.0.0.1 >NUL& echo TICK& ping -n 20 127.0.0.1 >NUL"
                        .to_string(),
                ],
            )
        } else {
            (
                "sh".to_string(),
                vec!["-c".to_string(), "echo READY; sleep 3; echo TICK; sleep 20".to_string()],
            )
        };
        PtyLaunch { program, args, cwd: None, env: BTreeMap::new(), rows: 24, cols: 80 }
    }

    #[test]
    fn a_session_survives_a_client_dropping_and_a_new_client_reattaches() {
        let daemon = SessionDaemon::new();
        daemon.launch("s1", &ready_then_tick()).unwrap();

        // Client A attaches, reads the initial output, then drops (simulating a UI/sidecar restart).
        let a = daemon.attach("s1").unwrap();
        let seen_a = drain_attach(&a, "READY", Duration::from_secs(10));
        assert!(seen_a.contains("READY"), "client A missed READY: {seen_a:?}");
        drop(a); // the UI process went away

        // The session must still be running — it is owned by the daemon, not the client.
        assert!(daemon.is_running("s1"), "session died when its client dropped");

        // Client B re-attaches: it must recover what it missed (READY) and then get subsequent
        // output (TICK) — from replay or live, whichever the timing produced.
        let b = daemon.attach("s1").unwrap();
        assert!(
            String::from_utf8_lossy(&b.replay).contains("READY"),
            "reattach did not replay missed output: {:?}",
            String::from_utf8_lossy(&b.replay)
        );
        let seen_b = drain_attach(&b, "TICK", Duration::from_secs(15));
        assert!(seen_b.contains("TICK"), "reattached client got no live output: {seen_b:?}");

        daemon.kill("s1").unwrap();
    }

    #[test]
    fn two_clients_attached_at_once_both_get_output() {
        let daemon = SessionDaemon::new();
        daemon.launch("s2", &ready_then_tick()).unwrap();
        let a = daemon.attach("s2").unwrap();
        let b = daemon.attach("s2").unwrap();
        assert!(drain_attach(&a, "READY", Duration::from_secs(10)).contains("READY"));
        assert!(drain_attach(&b, "READY", Duration::from_secs(10)).contains("READY"));
        daemon.kill("s2").unwrap();
    }

    // Unix-only: `cat` deterministically echoes each stdin line. The Windows equivalent
    // (ConPTY terminal echo + `cmd` quoting) is flaky to assert on; input routing itself is
    // OS-agnostic (`PtySession::writer` → `take_writer`, the same path the console server uses).
    #[cfg(unix)]
    #[test]
    fn input_reaches_the_pty_and_echoes_back() {
        // A filter that re-emits each stdin line proves keystrokes from a (re)attached client
        // route through to the PTY. We feed a pre-marked line and look for it coming back out.
        let (program, args) = ("cat".to_string(), Vec::<String>::new());
        let daemon = SessionDaemon::new();
        daemon
            .launch("s3", &PtyLaunch { program, args, cwd: None, env: BTreeMap::new(), rows: 24, cols: 80 })
            .unwrap();
        let att = daemon.attach("s3").unwrap();
        daemon.input("s3", b"GOT-hello\n").unwrap();
        let out = recv_until(&att.live, "GOT-hello", Duration::from_secs(10));
        assert!(out.contains("GOT-hello"), "input did not echo: {out:?}");
        let _ = daemon.kill("s3");
    }

    #[test]
    fn reaping_drops_exited_sessions_and_list_reflects_it() {
        let daemon = SessionDaemon::new();
        let (program, args) = crate::pty::shell_command("echo bye");
        daemon
            .launch("s4", &PtyLaunch { program, args, cwd: None, env: BTreeMap::new(), rows: 24, cols: 80 })
            .unwrap();
        // Observe the output (replay or live), then wait for the real exit below.
        let att = daemon.attach("s4").unwrap();
        let _ = drain_attach(&att, "bye", Duration::from_secs(10));
        // Wait until the child has actually exited (try_wait — reliable on every OS, unlike
        // reader EOF which a Windows ConPTY may withhold).
        let deadline = Instant::now() + Duration::from_secs(15);
        while daemon.is_running("s4") && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!daemon.is_running("s4"), "session did not exit");
        assert_eq!(daemon.list(), vec!["s4".to_string()], "session should still be held until reaped");
        assert_eq!(daemon.reap_exited(), vec!["s4".to_string()]);
        assert!(daemon.list().is_empty(), "reaped session still listed");
    }

    #[test]
    fn attaching_to_an_unknown_session_errors() {
        let daemon = SessionDaemon::new();
        assert!(daemon.attach("nope").is_err());
        assert!(!daemon.is_running("nope"));
    }

    #[test]
    fn close_all_kills_and_clears_every_session_and_is_idempotent(){
        let daemon = SessionDaemon::new();
        for id in ["a", "b", "c"] {
            daemon.launch(id, &ready_then_tick()).unwrap();
        }
        assert_eq!(daemon.list(), vec!["a", "b", "c"]);
        // Every session is live before teardown.
        assert!(daemon.is_running("a") && daemon.is_running("b") && daemon.is_running("c"));

        // One call reclaims them all.
        assert_eq!(daemon.close_all(), vec!["a", "b", "c"]);
        assert!(daemon.list().is_empty(), "sessions remained after close_all");
        assert!(!daemon.is_running("a") && !daemon.is_running("b") && !daemon.is_running("c"));

        // Idempotent: closing again with nothing held is a no-op.
        assert!(daemon.close_all().is_empty());
    }
}
