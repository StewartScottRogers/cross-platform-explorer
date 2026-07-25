//! CPE-309 slice 2: the `--session-daemon` entry runs as its **own process**, binds a loopback
//! socket, announces its port, and serves the session protocol. The reattach behaviour itself is
//! unit-tested in-process (`session_server`); this proves the separate-process wiring is real.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn the_session_daemon_process_boots_binds_and_serves_the_protocol() {
    // Launch the built binary in daemon mode; it prints `PORT <n>` once it's listening.
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

    // Connect over the real socket and speak the line protocol: `list` must answer with an empty set.
    let mut sock = TcpStream::connect(("127.0.0.1", port)).expect("connect daemon socket");
    sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    sock.write_all(b"{\"op\":\"list\"}\n").unwrap();
    sock.flush().unwrap();

    let mut reader = BufReader::new(sock);
    let mut resp = String::new();
    reader.read_line(&mut resp).expect("read list response");
    let v: serde_json::Value = serde_json::from_str(resp.trim()).expect("json response");
    assert_eq!(v["ev"], "sessions", "got: {resp}");
    assert_eq!(v["ids"].as_array().unwrap().len(), 0);

    child.kill().expect("kill daemon");
    let _ = child.wait();
}
