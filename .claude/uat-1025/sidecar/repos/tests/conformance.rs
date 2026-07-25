//! CPE-432 (AC3): the real `repos` process passes the shared **contract conformance kit**.
//!
//! The kit lives in `sidecar-contract` (so a sidecar needs no host dependency — ADR 0001). Here we
//! spawn the built binary, wrap its stdio in a [`SidecarChannel`], and drive the whole battery:
//! well-formed Hello, schema version, version negotiation, reaches Ready, correlated responses, and
//! an error for an unknown method. The kit skips the sidecar's async `ui:<url>` announce.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use sidecar_contract::conformance::{run_conformance, SidecarChannel};
use sidecar_contract::{Envelope, Message, Request, CONTRACT_VERSION};

/// A [`SidecarChannel`] over the child's JSON-lines stdio, with a bounded `recv` so a misbehaving
/// sidecar can never hang the test. A reader thread decodes lines onto a channel.
struct ProcChannel {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Result<Envelope, String>>,
}

impl ProcChannel {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_repos"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn repos process");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(l) if l.trim().is_empty() => continue,
                    Ok(l) => {
                        let decoded = Envelope::from_json(l.trim()).map_err(|e| e.to_string());
                        if tx.send(decoded).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Self { child, stdin, rx }
    }
}

impl SidecarChannel for ProcChannel {
    fn send(&mut self, env: &Envelope) -> Result<(), String> {
        let line = env.to_json().map_err(|e| e.to_string())?;
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| e.to_string())
    }
    fn recv(&mut self) -> Result<Envelope, String> {
        match self.rx.recv_timeout(Duration::from_secs(5)) {
            Ok(res) => res,
            Err(mpsc::RecvTimeoutError::Timeout) => Err("recv timed out".into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err("sidecar closed".into()),
        }
    }
}

#[test]
fn the_repos_process_passes_the_conformance_kit() {
    let mut chan = ProcChannel::spawn();

    let report = run_conformance(&mut chan, CONTRACT_VERSION);
    assert!(
        report.passed(),
        "conformance failures: {:?}",
        report.failures().collect::<Vec<_>>()
    );

    // Clean shutdown: ask it to exit and confirm code 0.
    let shutdown = Envelope::new(
        9_999,
        Message::Request(Request { method: "sidecar.shutdown".into(), params: serde_json::Value::Null }),
    );
    chan.send(&shutdown).expect("send shutdown");
    assert_eq!(chan.child.wait().expect("wait").code(), Some(0));
}
