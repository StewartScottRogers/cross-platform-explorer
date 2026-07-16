//! CPE-432: the repos sidecar runs as its **own process** — it emits `Hello` on start, reaches
//! `Ready` after `Welcome`, and exits cleanly on `sidecar.shutdown`. The reaction logic is
//! unit-tested in `protocol`; this proves the real stdio process wiring speaks the contract.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};

use sidecar_contract::{
    Capability, Envelope, Event, Lifecycle, Message, Request, Welcome, CONTRACT_VERSION,
};

#[test]
fn the_repos_process_boots_handshakes_and_shuts_down() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_repos"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn repos process");

    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    // 1. On start the process announces itself with a Hello.
    let mut line = String::new();
    out.read_line(&mut line).expect("read Hello line");
    let hello = Envelope::from_json(line.trim()).expect("parse Hello");
    match hello.message {
        Message::Hello(h) => {
            assert_eq!(h.sidecar_id, "repos");
            assert!(h.capabilities_requested.contains(&Capability::Network));
        }
        other => panic!("expected Hello, got {other:?}"),
    }

    // 2. Send Welcome; the process must reach Ready.
    let welcome = Envelope::new(
        0,
        Message::Welcome(Welcome {
            negotiated_version: CONTRACT_VERSION,
            capabilities_granted: vec![Capability::Network],
        }),
    );
    writeln!(stdin, "{}", welcome.to_json().unwrap()).unwrap();
    stdin.flush().unwrap();

    line.clear();
    out.read_line(&mut line).expect("read Ready line");
    let ready = Envelope::from_json(line.trim()).expect("parse Ready");
    assert!(matches!(ready.message, Message::Lifecycle(Lifecycle::Ready)), "got: {line}");

    // 3. A shutdown request ends the process cleanly.
    let shutdown = Envelope::new(
        9,
        Message::Request(Request { method: "sidecar.shutdown".into(), params: serde_json::Value::Null }),
    );
    writeln!(stdin, "{}", shutdown.to_json().unwrap()).unwrap();
    stdin.flush().unwrap();

    let status = child.wait().expect("wait for repos process");
    assert_eq!(status.code(), Some(0), "process should exit 0 on shutdown");
}

#[test]
fn the_repos_process_serves_its_ui_and_announces_the_url() {
    // CPE-432 AC2: after Welcome, the sidecar reaches Ready, serves its own loopback UI, and
    // announces the URL to the host via a `ui:<url>` Status event. The host embeds that URL.
    let mut child = Command::new(env!("CARGO_BIN_EXE_repos"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn repos process");

    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());

    // Consume the opening Hello.
    let mut line = String::new();
    out.read_line(&mut line).expect("read Hello line");
    assert!(matches!(Envelope::from_json(line.trim()).unwrap().message, Message::Hello(_)));

    // Welcome → the process reaches Ready, then announces its UI URL.
    let welcome = Envelope::new(
        0,
        Message::Welcome(Welcome {
            negotiated_version: CONTRACT_VERSION,
            capabilities_granted: vec![Capability::Network],
        }),
    );
    writeln!(stdin, "{}", welcome.to_json().unwrap()).unwrap();
    stdin.flush().unwrap();

    // First frame after Welcome is Ready.
    line.clear();
    out.read_line(&mut line).expect("read Ready line");
    assert!(
        matches!(Envelope::from_json(line.trim()).unwrap().message, Message::Lifecycle(Lifecycle::Ready)),
        "expected Ready, got: {line}"
    );

    // Next frame is the `ui:<url>` announcement.
    line.clear();
    out.read_line(&mut line).expect("read UI announce line");
    let announced = match Envelope::from_json(line.trim()).unwrap().message {
        Message::Event(Event::Status { state }) => state,
        other => panic!("expected a Status ui announce, got {other:?}"),
    };
    let url = announced.strip_prefix("ui:").expect("announce is prefixed with ui:");
    assert!(url.starts_with("http://127.0.0.1:"), "loopback URL expected, got {url}");

    // The announced URL actually serves the placeholder page.
    let addr = url.trim_start_matches("http://");
    let mut stream = TcpStream::connect(addr).expect("connect to announced UI");
    stream.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();
    let mut resp = String::new();
    stream.read_to_string(&mut resp).unwrap();
    assert!(resp.contains("200 OK"), "resp: {resp}");
    assert!(resp.contains("Repositories"), "served page should be the repos UI, got: {resp}");

    // Clean shutdown.
    let shutdown = Envelope::new(
        9,
        Message::Request(Request { method: "sidecar.shutdown".into(), params: serde_json::Value::Null }),
    );
    writeln!(stdin, "{}", shutdown.to_json().unwrap()).unwrap();
    stdin.flush().unwrap();
    assert_eq!(child.wait().expect("wait").code(), Some(0));
}
