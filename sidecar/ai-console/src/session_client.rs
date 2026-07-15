//! Session daemon client (CPE-309 slice 3 foundation).
//!
//! The counterpart to [`crate::session_server`]: connects to a running session daemon over its
//! loopback socket and drives the line protocol, so a caller (the AI Console UI server — the eventual
//! `console.rs` swap) can `launch`/`attach`/`input`/`resize`/`kill`/`list` sessions that live in the
//! **daemon process**, not in the caller. Because the sessions outlive the connection, a UI process
//! that restarts just reconnects and `attach`es again — receiving the replay then live output. This
//! completes the reattach protocol on both ends (the server is proven by `session_server`'s tests).
//!
//! One background thread demultiplexes incoming lines: session I/O events (`replay`/`output`/`exit`)
//! are routed to the matching `attach` stream; control acks (`launched`/`ok`/`error`/`sessions`) go
//! to a control channel the request methods wait on. Requests are issued one at a time (the control
//! receiver is serialized), which suits the console's per-action calls.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use serde_json::{json, Value};

use crate::pty::PtyLaunch;

/// One message on an attached session's stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamMsg {
    /// Buffered scrollback the client missed, sent once on attach.
    Replay(Vec<u8>),
    /// A live output chunk.
    Output(Vec<u8>),
    /// The session's agent exited.
    Exit,
}

/// How long a request waits for its control ack before giving up.
const REQ_TIMEOUT: Duration = Duration::from_secs(30);

struct Inner {
    writer: Mutex<TcpStream>,
    ctrl: Mutex<Receiver<Value>>,
    streams: Mutex<HashMap<String, Sender<StreamMsg>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Unblock the reader thread's `read` so it observes the close and exits (no leaked thread/fd).
        if let Ok(w) = self.writer.lock() {
            let _ = w.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// A connection to a session daemon.
pub struct SessionClient {
    inner: Arc<Inner>,
}

fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}
fn unb64(v: &Value) -> Vec<u8> {
    v.get("data")
        .and_then(Value::as_str)
        .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
        .unwrap_or_default()
}

impl SessionClient {
    /// Connect to a daemon at `addr` (e.g. `127.0.0.1:PORT`) and start the demux reader.
    pub fn connect(addr: &str) -> std::io::Result<SessionClient> {
        let sock = TcpStream::connect(addr)?;
        let reader = BufReader::new(sock.try_clone()?);
        let (ctrl_tx, ctrl_rx) = mpsc::channel();
        let inner = Arc::new(Inner {
            writer: Mutex::new(sock),
            ctrl: Mutex::new(ctrl_rx),
            streams: Mutex::new(HashMap::new()),
        });
        // The reader holds a WEAK ref so a dropped `SessionClient` lets `Inner` drop; its `Drop`
        // shuts the socket down, which unblocks this `read` and ends the thread (no leak).
        let weak = Arc::downgrade(&inner);
        std::thread::spawn(move || {
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
                let Some(inner) = weak.upgrade() else { break }; // client dropped
                match v.get("ev").and_then(Value::as_str) {
                    Some("replay") | Some("output") | Some("exit") => route_stream(&inner, &v),
                    // launched / ok / error / sessions → a request ack (ignored if the client is gone).
                    Some(_) => {
                        let _ = ctrl_tx.send(v);
                    }
                    None => {}
                }
            }
        });
        Ok(SessionClient { inner })
    }

    /// Send a request and wait for its control ack. Returns the ack, or an error string (from an
    /// `{"ev":"error"}` reply or a timeout).
    fn request(&self, msg: Value) -> Result<Value, String> {
        {
            let mut w = self.inner.writer.lock().unwrap();
            writeln!(w, "{msg}").map_err(|e| e.to_string())?;
            w.flush().map_err(|e| e.to_string())?;
        }
        let ack = self
            .inner
            .ctrl
            .lock()
            .unwrap()
            .recv_timeout(REQ_TIMEOUT)
            .map_err(|_| "daemon did not respond".to_string())?;
        if ack.get("ev").and_then(Value::as_str) == Some("error") {
            return Err(ack.get("msg").and_then(Value::as_str).unwrap_or("error").to_string());
        }
        Ok(ack)
    }

    /// Launch a new session in the daemon.
    pub fn launch(&self, id: &str, launch: &PtyLaunch) -> Result<(), String> {
        let env: serde_json::Map<String, Value> =
            launch.env.iter().map(|(k, v)| (k.clone(), Value::from(v.clone()))).collect();
        self.request(json!({
            "op": "launch", "id": id, "program": launch.program, "args": launch.args,
            "cwd": launch.cwd, "env": env, "rows": launch.rows, "cols": launch.cols,
        }))
        .map(|_| ())
    }

    /// Attach to a session: returns a stream that yields the replay first, then live output until the
    /// session exits or the connection drops.
    pub fn attach(&self, id: &str) -> Result<Receiver<StreamMsg>, String> {
        let (tx, rx) = mpsc::channel();
        // Register BEFORE sending, so the replay (which the daemon sends immediately) is routed here.
        self.inner.streams.lock().unwrap().insert(id.to_string(), tx);
        let mut w = self.inner.writer.lock().unwrap();
        writeln!(w, "{}", json!({ "op": "attach", "id": id })).map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())?;
        Ok(rx)
    }

    /// Send input bytes to a session's PTY.
    pub fn input(&self, id: &str, bytes: &[u8]) -> Result<(), String> {
        self.request(json!({ "op": "input", "id": id, "data": b64(bytes) })).map(|_| ())
    }

    /// Resize a session's terminal.
    pub fn resize(&self, id: &str, rows: u16, cols: u16) -> Result<(), String> {
        self.request(json!({ "op": "resize", "id": id, "rows": rows, "cols": cols })).map(|_| ())
    }

    /// Kill a session.
    pub fn kill(&self, id: &str) -> Result<(), String> {
        self.request(json!({ "op": "kill", "id": id })).map(|_| ())
    }

    /// List the daemon's live session ids.
    pub fn list(&self) -> Result<Vec<String>, String> {
        let ack = self.request(json!({ "op": "list" }))?;
        Ok(ack
            .get("ids")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default())
    }
}

/// Route a stream event to the attached session's channel (dropping it if none is attached).
fn route_stream(inner: &Arc<Inner>, v: &Value) {
    let Some(id) = v.get("id").and_then(Value::as_str) else { return };
    let msg = match v.get("ev").and_then(Value::as_str) {
        Some("replay") => StreamMsg::Replay(unb64(v)),
        Some("output") => StreamMsg::Output(unb64(v)),
        Some("exit") => StreamMsg::Exit,
        _ => return,
    };
    let streams = inner.streams.lock().unwrap();
    if let Some(tx) = streams.get(id) {
        let _ = tx.send(msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_daemon::SessionDaemon;
    use crate::session_server::{bind, serve};
    use std::collections::BTreeMap;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Instant;

    fn start() -> String {
        let listener = bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        std::thread::spawn(move || serve(listener, Arc::new(SessionDaemon::new())));
        addr
    }

    fn ready_then_tick() -> PtyLaunch {
        let (program, args) = if cfg!(target_os = "windows") {
            (
                "cmd".to_string(),
                vec![
                    "/c".to_string(),
                    "echo READY& ping -n 4 127.0.0.1 >NUL& echo TICK& ping -n 20 127.0.0.1 >NUL".to_string(),
                ],
            )
        } else {
            ("sh".to_string(), vec!["-c".to_string(), "echo READY; sleep 3; echo TICK; sleep 20".to_string()])
        };
        PtyLaunch { program, args, cwd: None, env: BTreeMap::new(), rows: 24, cols: 80 }
    }

    /// Drain a stream, accumulating decoded bytes, until `marker` is present or the deadline passes.
    fn drain_until(rx: &Receiver<StreamMsg>, marker: &str, timeout: Duration) -> String {
        let mut seen = String::new();
        let deadline = Instant::now() + timeout;
        while !seen.contains(marker) {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else { break };
            match rx.recv_timeout(remaining.min(Duration::from_millis(300))) {
                Ok(StreamMsg::Replay(b)) | Ok(StreamMsg::Output(b)) => seen.push_str(&String::from_utf8_lossy(&b)),
                Ok(StreamMsg::Exit) => break,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        seen
    }

    #[test]
    fn client_launches_attaches_lists_and_kills() {
        let addr = start();
        let c = SessionClient::connect(&addr).unwrap();
        c.launch("s1", &ready_then_tick()).unwrap();
        assert_eq!(c.list().unwrap(), vec!["s1".to_string()]);
        let rx = c.attach("s1").unwrap();
        assert!(drain_until(&rx, "READY", Duration::from_secs(10)).contains("READY"));
        c.kill("s1").unwrap();
    }

    #[test]
    fn a_second_client_reattaches_and_gets_replay_plus_live_output() {
        let addr = start();
        // Client 1 launches and reads the first output, then drops (UI restart).
        let c1 = SessionClient::connect(&addr).unwrap();
        c1.launch("s1", &ready_then_tick()).unwrap();
        let rx1 = c1.attach("s1").unwrap();
        assert!(drain_until(&rx1, "READY", Duration::from_secs(10)).contains("READY"));
        drop(c1);

        // Client 2 reconnects: the session is still live, and re-attach replays READY then streams TICK.
        let c2 = SessionClient::connect(&addr).unwrap();
        assert!(c2.list().unwrap().contains(&"s1".to_string()), "session died with client 1");
        let rx2 = c2.attach("s1").unwrap();
        let seen = drain_until(&rx2, "TICK", Duration::from_secs(20));
        assert!(seen.contains("READY"), "reattach lost the replay: {seen:?}");
        assert!(seen.contains("TICK"), "reattach got no live output: {seen:?}");
        c2.kill("s1").unwrap();
    }

    #[test]
    fn errors_surface_as_err() {
        let addr = start();
        let c = SessionClient::connect(&addr).unwrap();
        assert!(c.kill("nope").is_err()); // unknown session
    }
}
