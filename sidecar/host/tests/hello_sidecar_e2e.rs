//! End-to-end tests for the reference `hello_sidecar` (CPE-273) against a REAL child
//! process. This is the point of the ticket: spawn the actual binary and drive it
//! through the whole platform — handshake, the capability broker with all four
//! providers wired up (Context / Secrets / Storage / Events), and the event router —
//! proving a sidecar can exercise every capability over a genuine OS process boundary.
//!
//! The secrets backend here is a purpose-built in-memory store — the tests never touch
//! the real OS keychain. All waits are bounded so a misbehaving sidecar can never hang
//! the suite.

use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sidecar_contract::{
    Capability, Envelope, Event, Level, Message, Request, CONTRACT_VERSION,
};
use sidecar_host::broker::Broker;
use sidecar_host::conformance::{run_conformance, SidecarChannel};
use sidecar_host::providers::{
    ContextProvider, ContextSnapshot, ContextSource, EventRouter, EventSink, SecretBackend,
    SecretsProvider, StorageProvider,
};
use sidecar_host::supervisor::{handshake, spawn_process, Connection};

/// Path to the compiled hello_sidecar binary (Cargo sets this for bin targets).
fn hello_bin() -> String {
    env!("CARGO_BIN_EXE_hello_sidecar").to_string()
}

fn all_caps() -> BTreeSet<Capability> {
    [
        Capability::Context,
        Capability::Secrets,
        Capability::Storage,
        Capability::Events,
    ]
    .into_iter()
    .collect()
}

/// Keychain "service" the SecretsProvider derives for a sidecar id (kept in sync with
/// `providers::secrets::SecretsProvider::service_for`).
fn service_for(sidecar_id: &str) -> String {
    format!("com.cross-platform-explorer.sidecar.{sidecar_id}")
}

// ---------------------------------------------------------------------------
// Test-only host fakes (never the real keychain / explorer)
// ---------------------------------------------------------------------------

/// A fixed explorer-context source for the Context capability.
struct FixedContext;
impl ContextSource for FixedContext {
    fn snapshot(&self) -> ContextSnapshot {
        ContextSnapshot {
            folder: Some("/repo/src".into()),
            repo_root: Some("/repo".into()),
            remote: Some("https://example.com/repo".into()),
            selection: vec!["/repo/src/main.rs".into()],
        }
    }
}

/// An in-memory secrets store, shared by clone (never the OS keychain). Keyed by
/// (service, account) exactly like the real backend.
#[derive(Clone, Default)]
struct MemBackend {
    map: Arc<Mutex<HashMap<(String, String), String>>>,
}
impl SecretBackend for MemBackend {
    fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
        self.map
            .lock()
            .unwrap()
            .insert((service.into(), account.into()), secret.into());
        Ok(())
    }
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        Ok(self
            .map
            .lock()
            .unwrap()
            .get(&(service.into(), account.into()))
            .cloned())
    }
    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        self.map.lock().unwrap().remove(&(service.into(), account.into()));
        Ok(())
    }
}

/// Records every event the sidecar emits, shared by clone.
#[derive(Clone, Default)]
struct RecordingSink {
    events: Arc<Mutex<Vec<String>>>,
}
impl RecordingSink {
    fn recorded(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}
impl EventSink for RecordingSink {
    fn notify(&self, sidecar_id: &str, level: Level, message: &str) {
        self.events
            .lock()
            .unwrap()
            .push(format!("notify:{sidecar_id}:{level:?}:{message}"));
    }
    fn progress(&self, sidecar_id: &str, id: &str, fraction: f32) {
        self.events
            .lock()
            .unwrap()
            .push(format!("progress:{sidecar_id}:{id}:{fraction}"));
    }
    fn status(&self, sidecar_id: &str, state: &str) {
        self.events
            .lock()
            .unwrap()
            .push(format!("status:{sidecar_id}:{state}"));
    }
}

/// Build a broker with all four providers registered and every capability granted to
/// `hello`. Returns the broker plus handles to inspect the secrets store afterwards.
fn build_broker(storage_base: &std::path::Path, secrets: MemBackend) -> Broker {
    let mut broker = Broker::new();
    broker.register_provider(Box::new(ContextProvider::new(FixedContext)));
    broker.register_provider(Box::new(SecretsProvider::new(secrets)));
    broker.register_provider(Box::new(StorageProvider::new(storage_base)));
    broker.set_grants("hello", all_caps());
    broker
}

/// Pump the sidecar↔host conversation: answer capability Requests through the broker,
/// route Events through the router, and return once the sidecar emits
/// `Event::Status { state: "done" }`. Bounded by `deadline` so it can never hang.
fn run_until_done(
    conn: &mut dyn Connection,
    broker: &Broker,
    router: &EventRouter<RecordingSink>,
    deadline: Instant,
) {
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for the sidecar to finish");
        let env = conn.recv().expect("recv from sidecar");
        match env.message {
            Message::Request(req) => {
                let resp = broker.dispatch("hello", &req);
                conn.send(&Envelope::new(env.id, Message::Response(resp)))
                    .expect("send response");
            }
            Message::Event(ev) => {
                assert!(router.deliver("hello", true, &ev), "granted event should route");
                if let Event::Status { state } = &ev {
                    if state == "done" {
                        return;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Ask a sidecar to shut down cleanly (it exits 0 on this request).
fn shutdown_request(conn: &mut dyn Connection) {
    let _ = conn.send(&Envelope::new(
        9_999,
        Message::Request(Request {
            method: "sidecar.shutdown".into(),
            params: serde_json::Value::Null,
        }),
    ));
}

// ---------------------------------------------------------------------------
// Test 1 — the full capability tour over a real process (the ticket's core).
// ---------------------------------------------------------------------------

#[test]
fn hello_exercises_all_four_capabilities_over_a_real_process() {
    let storage_base = tempfile::tempdir().expect("temp dir");
    let secrets = MemBackend::default();
    let sink = RecordingSink::default();

    let broker = build_broker(storage_base.path(), secrets.clone());
    let router = EventRouter::new(sink.clone());

    let mut conn = spawn_process(&hello_bin(), &[]).expect("spawn hello_sidecar");

    // Handshake, consenting to (and thus granting) all four capabilities.
    let outcome = { let _tok = conn.launch_token().to_string(); handshake(&mut conn, CONTRACT_VERSION, &all_caps(), Some(&_tok)) }.expect("handshake");
    assert_eq!(outcome.sidecar_id, "hello");
    assert_eq!(outcome.granted, all_caps(), "all four capabilities granted");

    let deadline = Instant::now() + Duration::from_secs(20);
    run_until_done(&mut conn, &broker, &router, deadline);

    // --- Assert every capability was actually exercised over the wire ---

    // Events: the sidecar's notify + terminal "done" status reached the sink.
    let recorded = sink.recorded();
    assert!(
        recorded.iter().any(|e| e.starts_with("notify:hello:Info:")),
        "expected an Info notify, got {recorded:?}"
    );
    assert!(
        recorded.iter().any(|e| e == "status:hello:done"),
        "expected the done status, got {recorded:?}"
    );

    // Secrets: the value the sidecar set round-tripped into OUR in-memory backend.
    let stored = secrets
        .get(&service_for("hello"), "hello-token")
        .expect("backend get");
    assert_eq!(stored.as_deref(), Some("s3cr3t-value"), "secret was set via the broker");

    // Storage: the sidecar's private namespace dir was created under the temp base.
    let ns = storage_base.path().join("hello");
    assert!(ns.is_dir(), "storage namespace dir should exist at {}", ns.display());

    // (Context was exercised too: the tour's context.current call is dispatched through
    // the broker inside run_until_done; a broker error there would have surfaced as a
    // sidecar Event::Notify{Error} instead of the clean "done" we required above.)

    // Clean up: ask the sidecar to exit, then confirm the process is gone.
    shutdown_request(&mut conn);
    let gone_by = Instant::now() + Duration::from_secs(5);
    while conn.is_alive() && Instant::now() < gone_by {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(!conn.is_alive(), "sidecar should exit after shutdown");
}

// ---------------------------------------------------------------------------
// Test 2 — the sidecar exits 0 on a clean shutdown.
//
// spawn_process's ProcessConnection intentionally hides the child's exit code, so this
// test drives the same binary through a purpose-built RawChild connection that owns the
// Child and can read its exit status. RawChild uses a reader thread + timeout, mirroring
// ProcessConnection, so recv can never block forever.
// ---------------------------------------------------------------------------

/// A [`Connection`] that keeps ownership of the child so the test can wait on its exit
/// status. Same JSON-line stdio transport as the production `ProcessConnection`.
struct RawChild {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Result<Envelope, String>>,
}

impl RawChild {
    fn spawn(bin: &str) -> Self {
        let mut child = Command::new(bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn hello_sidecar (raw)");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let buf = BufReader::new(stdout);
            for line in buf.lines() {
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

    /// Wait up to `dur` for the child to exit, returning its exit code (or `None` on
    /// timeout, after force-killing it).
    fn wait_for_exit(&mut self, dur: Duration) -> Option<i32> {
        let deadline = Instant::now() + dur;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return status.code(),
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = self.child.kill();
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(_) => return None,
            }
        }
    }
}

impl SidecarChannel for RawChild {
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
impl Connection for RawChild {
    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
    fn shutdown(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn hello_exits_zero_on_clean_shutdown() {
    let storage_base = tempfile::tempdir().expect("temp dir");
    let secrets = MemBackend::default();
    let sink = RecordingSink::default();
    let broker = build_broker(storage_base.path(), secrets);
    let router = EventRouter::new(sink);

    let mut conn = RawChild::spawn(&hello_bin());
    // RawChild spawns without issuing a token, so the sidecar echoes none → pass None.
    let outcome = handshake(&mut conn, CONTRACT_VERSION, &all_caps(), None).expect("handshake");
    assert_eq!(outcome.granted, all_caps());

    let deadline = Instant::now() + Duration::from_secs(20);
    run_until_done(&mut conn, &broker, &router, deadline);

    // Ask it to shut down; it must exit with code 0.
    shutdown_request(&mut conn);
    let code = conn.wait_for_exit(Duration::from_secs(5));
    assert_eq!(code, Some(0), "hello_sidecar should exit 0 on clean shutdown");
}

// ---------------------------------------------------------------------------
// Test 3 — hello still satisfies the base protocol (conformance kit, CPE-301).
//
// The kit grants NO capabilities, so hello runs no proactive capability tour and
// behaves as a passive request/response sidecar — it must still pass every check.
// ---------------------------------------------------------------------------

#[test]
fn conformance_kit_passes_against_hello() {
    let mut conn = spawn_process(&hello_bin(), &[]).expect("spawn hello_sidecar");
    let report = run_conformance(&mut conn, CONTRACT_VERSION);
    assert!(
        report.passed(),
        "conformance failures: {:?}",
        report.failures().collect::<Vec<_>>()
    );
    conn.shutdown();
}
