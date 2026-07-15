//! Session daemon socket server (CPE-309 slice 2).
//!
//! Slice 1 ([`crate::session_daemon`]) made agent PTY sessions outlive any attached client. This
//! slice runs a [`SessionDaemon`] in its **own long-lived process** behind a loopback TCP socket, so
//! the AI Console UI server (a separate, restartable process — slice 3) can connect, drop, and
//! **re-connect** to a still-running agent. The wire protocol is one JSON object per line
//! (newline-delimited), with binary PTY bytes base64-encoded:
//!
//! - client → daemon: `{"op":"launch",…}` `{"op":"attach","id":…}` `{"op":"input","id":…,"data":b64}`
//!   `{"op":"resize",…}` `{"op":"kill","id":…}` `{"op":"list"}`
//! - daemon → client: `{"ev":"launched","id":…}` `{"ev":"replay","id":…,"data":b64}`
//!   `{"ev":"output","id":…,"data":b64}` `{"ev":"exit","id":…}` `{"ev":"sessions","ids":[…]}`
//!   `{"ev":"ok"}` `{"ev":"error","msg":…}`
//!
//! On `attach` the daemon writes the buffered **replay** then streams live **output** — the reattach
//! that restores I/O to a reconnecting UI. The socket is loopback-only (same trust boundary as the
//! sidecar's own UI server, CPE-271); it carries terminal I/O, never secrets.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use serde_json::{json, Value};

use crate::pty::PtyLaunch;
use crate::session_daemon::SessionDaemon;

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
fn unb64(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

/// Bind the daemon's loopback listener. Port 0 lets the OS pick; the caller reads `local_addr`.
pub fn bind(addr: &str) -> std::io::Result<TcpListener> {
    TcpListener::bind(addr)
}

/// Serve the line protocol on `listener`, backed by `daemon`, until the listener errors/closes.
/// Each accepted connection is handled on its own thread; `daemon` is shared so sessions persist
/// across connections. Blocking — run it on a dedicated thread (or as the daemon process's main).
pub fn serve(listener: TcpListener, daemon: Arc<SessionDaemon>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let daemon = Arc::clone(&daemon);
        std::thread::spawn(move || handle_client(stream, daemon));
    }
}

/// A single client connection: read one JSON op per line and act on it; an `attach` also spawns a
/// pump thread that streams that session's live output to this socket.
fn handle_client(stream: TcpStream, daemon: Arc<SessionDaemon>) {
    let reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(_) => return,
    };
    // One shared writer: op-replies and every attached session's output pump serialize through it,
    // so lines never interleave.
    let writer = Arc::new(Mutex::new(stream));
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            send(&writer, json!({ "ev": "error", "msg": "bad json" }));
            continue;
        };
        if !dispatch(&msg, &daemon, &writer) {
            break; // fatal write error — client gone
        }
    }
}

/// Handle one parsed op. Returns `false` only on a fatal socket write error.
fn dispatch(msg: &Value, daemon: &Arc<SessionDaemon>, writer: &Arc<Mutex<TcpStream>>) -> bool {
    let op = msg.get("op").and_then(Value::as_str).unwrap_or("");
    let id = || msg.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    match op {
        "launch" => {
            let launch = parse_launch(msg);
            match daemon.launch(&id(), &launch) {
                Ok(()) => send(writer, json!({ "ev": "launched", "id": id() })),
                Err(e) => send(writer, json!({ "ev": "error", "msg": e })),
            }
        }
        "attach" => match daemon.attach(&id()) {
            Ok(att) => {
                // Replay what the client missed, then pump live output on a background thread.
                if !send(writer, json!({ "ev": "replay", "id": id(), "data": b64(&att.replay) })) {
                    return false;
                }
                let w = Arc::clone(writer);
                let sid = id();
                std::thread::spawn(move || {
                    for chunk in att.live {
                        if !send(&w, json!({ "ev": "output", "id": sid, "data": b64(&chunk) })) {
                            return;
                        }
                    }
                    let _ = send(&w, json!({ "ev": "exit", "id": sid }));
                });
                true
            }
            Err(e) => send(writer, json!({ "ev": "error", "msg": e })),
        },
        "input" => {
            let bytes = msg.get("data").and_then(Value::as_str).and_then(unb64).unwrap_or_default();
            reply_ok(writer, daemon.input(&id(), &bytes))
        }
        "resize" => {
            let rows = msg.get("rows").and_then(Value::as_u64).unwrap_or(24) as u16;
            let cols = msg.get("cols").and_then(Value::as_u64).unwrap_or(80) as u16;
            reply_ok(writer, daemon.resize(&id(), rows, cols))
        }
        "kill" => reply_ok(writer, daemon.kill(&id())),
        "list" => send(writer, json!({ "ev": "sessions", "ids": daemon.list() })),
        other => send(writer, json!({ "ev": "error", "msg": format!("unknown op '{other}'") })),
    }
}

fn reply_ok(writer: &Arc<Mutex<TcpStream>>, r: Result<(), String>) -> bool {
    match r {
        Ok(()) => send(writer, json!({ "ev": "ok" })),
        Err(e) => send(writer, json!({ "ev": "error", "msg": e })),
    }
}

/// Build a [`PtyLaunch`] from a `launch` message (missing fields get sane defaults).
fn parse_launch(msg: &Value) -> PtyLaunch {
    let str_of = |k: &str| msg.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let args = msg
        .get("args")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let env: BTreeMap<String, String> = msg
        .get("env")
        .and_then(Value::as_object)
        .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
        .unwrap_or_default();
    PtyLaunch {
        program: str_of("program"),
        args,
        cwd: msg.get("cwd").and_then(Value::as_str).map(str::to_string),
        env,
        rows: msg.get("rows").and_then(Value::as_u64).unwrap_or(24) as u16,
        cols: msg.get("cols").and_then(Value::as_u64).unwrap_or(80) as u16,
    }
}

/// Write one JSON line to the socket. Returns `false` on a write error (client gone).
fn send(writer: &Arc<Mutex<TcpStream>>, v: Value) -> bool {
    let mut w = writer.lock().unwrap();
    writeln!(w, "{v}").and_then(|_| w.flush()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
    use std::time::Duration;

    /// Start a daemon server on an ephemeral loopback port; return its address.
    fn start() -> String {
        let listener = bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let daemon = Arc::new(SessionDaemon::new());
        std::thread::spawn(move || serve(listener, daemon));
        addr
    }

    /// A tiny line client over a real TCP socket. A background thread reads complete lines
    /// (blocking, so a line is never split by a timeout) and forwards parsed JSON over a channel.
    struct Client {
        w: TcpStream,
        rx: Receiver<Value>,
    }
    impl Client {
        fn connect(addr: &str) -> Client {
            let w = TcpStream::connect(addr).unwrap();
            let r = BufReader::new(w.try_clone().unwrap());
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                for line in r.lines() {
                    let Ok(line) = line else { break };
                    if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                        if tx.send(v).is_err() {
                            break;
                        }
                    }
                }
            });
            Client { w, rx }
        }
        fn send(&mut self, v: Value) {
            writeln!(self.w, "{v}").unwrap();
            self.w.flush().unwrap();
        }
        /// Drain forwarded messages until one matches `pred` or the deadline passes.
        fn wait_for(&mut self, timeout: Duration, mut pred: impl FnMut(&Value) -> bool) -> Option<Value> {
            let deadline = std::time::Instant::now() + timeout;
            loop {
                let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
                match self.rx.recv_timeout(remaining.min(Duration::from_millis(500))) {
                    Ok(v) if pred(&v) => return Some(v),
                    Ok(_) => {}
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return None,
                }
            }
        }
    }

    fn ev_is(v: &Value, ev: &str) -> bool {
        v.get("ev").and_then(Value::as_str) == Some(ev)
    }
    /// A `marker` seen in the decoded data of a `replay` or `output` event (either is a valid way to
    /// receive it, depending on PTY-vs-attach timing).
    fn saw_marker(v: &Value, marker: &str) -> bool {
        (ev_is(v, "replay") || ev_is(v, "output")) && decode_data(v).contains(marker)
    }
    fn decode_data(v: &Value) -> String {
        v.get("data")
            .and_then(Value::as_str)
            .and_then(unb64)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }

    /// A command that prints READY, waits, prints TICK, then stays alive (cross-platform).
    fn ready_then_tick() -> Value {
        if cfg!(target_os = "windows") {
            json!({ "op": "launch", "id": "s1", "program": "cmd", "args": ["/c",
                "echo READY& ping -n 4 127.0.0.1 >NUL& echo TICK& ping -n 20 127.0.0.1 >NUL"] })
        } else {
            json!({ "op": "launch", "id": "s1", "program": "sh", "args": ["-c",
                "echo READY; sleep 3; echo TICK; sleep 20"] })
        }
    }

    #[test]
    fn a_reconnecting_client_replays_and_resumes_over_the_socket() {
        let addr = start();

        // Client A launches a session and reads its initial output, then DISCONNECTS (UI restart).
        // READY may arrive as live `output` or, if the child emitted it before we attached, as the
        // `replay` — accept either so the test isn't racing PTY vs socket timing.
        let mut a = Client::connect(&addr);
        a.send(ready_then_tick());
        assert!(a.wait_for(Duration::from_secs(5), |v| ev_is(v, "launched")).is_some());
        a.send(json!({ "op": "attach", "id": "s1" }));
        let got = a.wait_for(Duration::from_secs(10), |v| saw_marker(v, "READY"));
        assert!(got.is_some(), "client A never saw READY");
        drop(a); // the UI process went away — the SESSION must keep running in the daemon

        // Client B reconnects: the session must still be alive in the daemon…
        let mut b = Client::connect(&addr);
        b.send(json!({ "op": "list" }));
        let sessions = b.wait_for(Duration::from_secs(5), |v| ev_is(v, "sessions")).unwrap();
        assert!(sessions["ids"].as_array().unwrap().iter().any(|x| x == "s1"), "session died with client A");

        // …and on re-attach B must recover BOTH what it missed (READY, via replay) and subsequent
        // LIVE output (TICK) — the reattach that restores I/O to a restarted UI. Accumulate across
        // replay+output so the exact replay/output split never matters.
        b.send(json!({ "op": "attach", "id": "s1" }));
        let mut seen = String::new();
        b.wait_for(Duration::from_secs(20), |v| {
            if ev_is(v, "replay") || ev_is(v, "output") {
                seen.push_str(&decode_data(v));
            }
            seen.contains("READY") && seen.contains("TICK")
        });
        assert!(seen.contains("READY"), "reattach did not replay missed output: {seen:?}");
        assert!(seen.contains("TICK"), "reattached client got no live output: {seen:?}");

        b.send(json!({ "op": "kill", "id": "s1" }));
        assert!(b.wait_for(Duration::from_secs(5), |v| ev_is(v, "ok")).is_some());
    }

    #[test]
    fn bad_ops_and_unknown_sessions_report_errors_not_crashes() {
        let addr = start();
        let mut c = Client::connect(&addr);
        c.send(json!({ "op": "attach", "id": "nope" }));
        assert!(c.wait_for(Duration::from_secs(5), |v| ev_is(v, "error")).is_some());
        c.send(json!({ "op": "frobnicate" }));
        assert!(c.wait_for(Duration::from_secs(5), |v| ev_is(v, "error")).is_some());
        // Malformed (non-JSON) line is reported, and the connection stays usable.
        writeln!(c.w, "not json").unwrap();
        c.w.flush().unwrap();
        assert!(c.wait_for(Duration::from_secs(5), |v| ev_is(v, "error")).is_some());
        c.send(json!({ "op": "list" }));
        assert!(c.wait_for(Duration::from_secs(5), |v| ev_is(v, "sessions")).is_some());
    }
}
