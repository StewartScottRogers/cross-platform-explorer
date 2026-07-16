//! CPE-309 — the reattach-across-restart *mechanism*, proven end-to-end against the REAL
//! `--session-daemon` process (not an in-process served daemon).
//!
//! This is the automated form of the ticket's core claim ("restart the sidecar mid-session, assert
//! the session is recoverable"): the session's PTY lives in the daemon **process**, so when the
//! console/UI client that launched it goes away, the session keeps running and a *new* client can
//! reconnect, see it in `list`, and re-attach — receiving the buffered scrollback (replay) and then
//! live output. The daemon process is the thing that must survive a console restart, and here it is a
//! genuinely separate OS process.
//!
//! The full product close-out still needs the console-side rewire (route `ConsoleState` through the
//! `SessionClient`) + host-level supervision so the daemon outlives a sidecar *process* swap, and a
//! human GUI walkthrough — but the transport/engine mechanism those depend on is proven here.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use ai_console::session_client::{SessionClient, StreamMsg};
use ai_console::PtyLaunch;
use std::collections::BTreeMap;

/// Spawn the built binary in daemon mode and read the `PORT <n>` it announces once listening.
fn spawn_daemon() -> (Child, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ai-console"))
        .arg("--session-daemon")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn session-daemon process");
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).expect("read PORT line");
    let port: u16 = line
        .trim()
        .strip_prefix("PORT ")
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| panic!("expected `PORT <n>`, got {line:?}"));
    (child, format!("127.0.0.1:{port}"))
}

/// A command that prints READY, waits, prints TICK, then lingers — so we can read READY before the
/// first client drops and observe TICK arrive live on the reattached client.
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
fn a_session_survives_the_launching_client_dying_and_a_new_client_reattaches() {
    let (mut daemon, addr) = spawn_daemon();

    // Client A — the "console" that launches the agent — reads its first output, then DIES
    // (dropped), exactly as a sidecar UI process would on a restart/crash/update.
    let client_a = SessionClient::connect(&addr).expect("client A connects to daemon");
    client_a.launch("s1", &ready_then_tick()).expect("launch session in daemon");
    let rx_a = client_a.attach("s1").expect("client A attaches");
    assert!(
        drain_until(&rx_a, "READY", Duration::from_secs(10)).contains("READY"),
        "client A never saw the session's first output"
    );
    drop(client_a); // the launching console goes away — the session must NOT die with it

    // Client B — a fresh console after the "restart" — reconnects to the SAME daemon process.
    let client_b = SessionClient::connect(&addr).expect("client B reconnects to daemon");
    assert!(
        client_b.list().expect("list sessions").contains(&"s1".to_string()),
        "the session died when its launching client dropped — reattach is impossible"
    );
    let rx_b = client_b.attach("s1").expect("client B reattaches");
    let seen = drain_until(&rx_b, "TICK", Duration::from_secs(20));
    assert!(seen.contains("READY"), "reattach lost the scrollback replay: {seen:?}");
    assert!(seen.contains("TICK"), "reattach got no live output after reconnect: {seen:?}");

    client_b.kill("s1").expect("kill the recovered session");
    daemon.kill().expect("kill daemon");
    let _ = daemon.wait();
}
