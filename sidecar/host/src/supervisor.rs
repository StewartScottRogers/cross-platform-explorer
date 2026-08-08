//! Process supervisor (CPE-265) + version-negotiation enforcement (CPE-263).
//!
//! The supervisor owns a sidecar's lifecycle: it spawns the process, performs the
//! handshake (negotiating the contract version and **refusing to mount** an
//! incompatible sidecar — CPE-263), tracks liveness, and restarts on crash with
//! capped backoff. Each sidecar is its own crash domain: a sidecar dying never takes
//! down the host or another sidecar.
//!
//! The pure logic (handshake, restart policy) is tested with an in-memory fake
//! connection; the real [`ProcessConnection`] speaks JSON-line frames over the
//! child's stdio and is exercised end-to-end by `tests/supervisor_e2e.rs` against the
//! bundled `echo_sidecar` test binary.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;

use sidecar_contract::{
    negotiate, Capability, ContractVersion, Envelope, Lifecycle, Message, RejectCode, Rejected,
    VersionError, Welcome,
};

use crate::broker::{decide_grants, GrantRequest};
use crate::conformance::SidecarChannel;

/// Bound on buffered inbound envelopes before the reader applies backpressure (CPE-297).
const IPC_CHANNEL_CAPACITY: usize = 1024;

/// Bound on a single stdout line (one JSON envelope) the reader will buffer before giving up
/// on the connection (CPE-1471). The AI-Console sidecar runs arbitrary agent/model code (in
/// the threat model), so a compromised child that writes a lot of bytes without a newline must
/// not be able to grow the HOST process's memory without limit — `IPC_CHANNEL_CAPACITY` above
/// only bounds the *count* of buffered envelopes, not the size of any single one.
///
/// 16 MiB comfortably exceeds any legitimate envelope on this channel: `Request`/`Response`/
/// `Event` payloads are small control/status/tool-result JSON in practice, and it matches the
/// cap the app already applies at its other large-payload boundary,
/// `ai_console::http::MAX_REQUEST_BODY` (16 MiB) — so a real message that big would already be
/// rejected downstream anyway. A line beyond this is either a corrupt/hostile frame or noise
/// the child wrote without a newline; either way the right move is to sever the connection, not
/// grow without bound.
const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// Read one `\n`-terminated line from `r` into `buf` (bytes, newline stripped), refusing to
/// buffer past `cap` bytes (CPE-1471). Unlike `BufRead::lines()`/`read_until`, this never grows
/// `buf` beyond `cap`: it reads in `BufReader`-sized chunks via `fill_buf`/`consume` and bails
/// out the moment appending the next chunk would exceed the cap, so a line with no newline
/// anywhere in it is bounded rather than buffered in full. Returns `Ok(n)` (n = `buf.len()`) on
/// a completed line or clean EOF (n may be 0), or `Err` on an I/O error or a cap overflow.
fn read_bounded_line(r: &mut impl BufRead, buf: &mut Vec<u8>, cap: usize) -> Result<usize, String> {
    loop {
        let available = match r.fill_buf() {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("read: {e}")),
        };
        if available.is_empty() {
            return Ok(buf.len()); // EOF: whatever's buffered is the final (possibly empty) line.
        }
        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            buf.extend_from_slice(&available[..pos]);
            r.consume(pos + 1);
            return Ok(buf.len());
        }
        if buf.len() + available.len() > cap {
            return Err(format!("frame too large: line exceeds the {cap}-byte cap"));
        }
        let n = available.len();
        buf.extend_from_slice(available);
        r.consume(n);
    }
}

/// A live connection to a sidecar: the message channel plus process liveness/control.
pub trait Connection: SidecarChannel {
    fn is_alive(&mut self) -> bool;
    fn shutdown(&mut self);
}

// ---------------------------------------------------------------------------
// Handshake (CPE-262/263/266 integration point)
// ---------------------------------------------------------------------------

/// What a successful handshake established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeOutcome {
    pub sidecar_id: String,
    pub negotiated: ContractVersion,
    pub granted: BTreeSet<Capability>,
}

/// Why a handshake failed. `Version` is the "refuse to mount" path (CPE-263).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeError {
    NoHello(String),
    NotHello,
    Version(VersionError),
    NoReady(String),
    Send(String),
    /// Either the Hello's auth token didn't match the token the host issued at spawn
    /// (CPE-275), or its self-declared `sidecar_id` didn't match the id the host
    /// actually spawned (CPE-1472) — both signal a possible impostor connection.
    Untrusted,
}

/// A random per-launch channel token (CPE-275). The host sets it in the child's
/// environment (`AUTH_TOKEN_ENV`) at spawn; the sidecar echoes it in `Hello`, and the
/// host rejects a Hello whose token doesn't match — so a foreign process can't
/// impersonate a sidecar on the channel.
pub fn generate_launch_token() -> String {
    let mut bytes = [0u8; 16];
    // Fall back to a process/time-derived value only if the OS RNG is unavailable.
    if getrandom::getrandom(&mut bytes).is_err() {
        let seed = std::process::id() as u128
            ^ std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
        bytes = seed.to_le_bytes();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Drive the opening handshake over `conn`. On a major-version mismatch the host
/// sends a `Rejected` and returns `Version(..)` — it does **not** mount the sidecar.
/// `expected_id` must match the Hello's self-declared `sidecar_id` (CPE-1472): it is
/// the id the host actually spawned, never taken from the child's own say-so, so a
/// sidecar can't claim a different namespace's identity. When `expected_token` is
/// `Some`, the Hello's `auth_token` must also match it (CPE-275).
pub fn handshake(
    conn: &mut dyn Connection,
    host_version: ContractVersion,
    consented: &BTreeSet<Capability>,
    expected_id: &str,
    expected_token: Option<&str>,
) -> Result<HandshakeOutcome, HandshakeError> {
    let hello_env = conn.recv().map_err(HandshakeError::NoHello)?;
    let hello = match hello_env.message {
        Message::Hello(h) => h,
        _ => return Err(HandshakeError::NotHello),
    };

    // Authenticate the channel: the sidecar must echo the token we issued.
    if let Some(expected) = expected_token {
        if hello.auth_token.as_deref() != Some(expected) {
            let _ = conn.send(&Envelope::new(
                hello_env.id,
                Message::Rejected(Rejected {
                    code: RejectCode::Untrusted,
                    reason: "auth token mismatch".into(),
                }),
            ));
            return Err(HandshakeError::Untrusted);
        }
    }

    // The Hello's self-declared id must match what the host spawned (CPE-1472). Trusting
    // `hello.sidecar_id` unvalidated would let a child claim any identity; today the runtime
    // only ever spawns a hardcoded "ai-console"/"agent-board" so it isn't exploitable yet, but a
    // planned multi-sidecar host routes capability grants/consent/storage namespacing by this id
    // — an unvalidated claim there is a cross-namespace escalation.
    if hello.sidecar_id != expected_id {
        let _ = conn.send(&Envelope::new(
            hello_env.id,
            Message::Rejected(Rejected {
                code: RejectCode::Untrusted,
                reason: format!(
                    "sidecar id mismatch: expected '{expected_id}', got '{}'",
                    hello.sidecar_id
                ),
            }),
        ));
        return Err(HandshakeError::Untrusted);
    }

    let negotiated = match negotiate(host_version, hello.contract_version) {
        Ok(v) => v,
        Err(e) => {
            // Refuse to mount, with an actionable reason (CPE-263).
            let _ = conn.send(&Envelope::new(
                hello_env.id,
                Message::Rejected(Rejected {
                    code: RejectCode::IncompatibleVersion,
                    reason: format!("{e:?}"),
                }),
            ));
            return Err(HandshakeError::Version(e));
        }
    };

    let granted = decide_grants(&GrantRequest {
        requested: hello.capabilities_requested.clone(),
        consented: consented.clone(),
        policy_allow: None,
    });

    conn.send(&Envelope::new(
        hello_env.id,
        Message::Welcome(Welcome {
            negotiated_version: negotiated,
            capabilities_granted: granted.iter().copied().collect(),
        }),
    ))
    .map_err(HandshakeError::Send)?;

    match conn.recv().map_err(HandshakeError::NoReady)?.message {
        Message::Lifecycle(Lifecycle::Ready) => Ok(HandshakeOutcome {
            sidecar_id: hello.sidecar_id,
            negotiated,
            granted,
        }),
        other => Err(HandshakeError::NoReady(format!("expected Ready, got {other:?}"))),
    }
}

// ---------------------------------------------------------------------------
// Restart policy (pure)
// ---------------------------------------------------------------------------

/// Capped exponential backoff for crash restarts.
#[derive(Debug, Clone, Copy)]
pub struct RestartPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self { max_attempts: 5, base_delay_ms: 200, max_delay_ms: 5_000 }
    }
}

impl RestartPolicy {
    /// The delay before restart attempt `attempt` (0-based), or `None` once the cap
    /// on attempts is reached (give up — surface a failure).
    pub fn delay_for(&self, attempt: u32) -> Option<u64> {
        if attempt >= self.max_attempts {
            return None;
        }
        let shifted = self.base_delay_ms.saturating_mul(1u64 << attempt.min(20));
        Some(shifted.min(self.max_delay_ms))
    }
}

// ---------------------------------------------------------------------------
// Real process connection
// ---------------------------------------------------------------------------

/// A [`Connection`] backed by a child process speaking JSON-line frames over stdio.
pub struct ProcessConnection {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Result<Envelope, String>>,
    recv_timeout: Duration,
    launch_token: String,
    _reader: JoinHandle<()>,
}

/// Spawn `command args...` as a sidecar and wire its stdio into a [`ProcessConnection`].
/// A per-launch auth token is generated and passed to the child via `AUTH_TOKEN_ENV`;
/// the sidecar echoes it in `Hello` so [`handshake`] can authenticate the channel.
pub fn spawn_process(command: &str, args: &[String]) -> Result<ProcessConnection, String> {
    spawn_process_with_env(command, args, &[])
}

/// Like [`spawn_process`], but also sets extra environment variables on the child (CPE-376 —
/// e.g. the agent-catalog dir + trusted keys the sidecar loads).
pub fn spawn_process_with_env(
    command: &str,
    args: &[String],
    env: &[(&str, &str)],
) -> Result<ProcessConnection, String> {
    let launch_token = generate_launch_token();
    let mut cmd = Command::new(command);
    cmd.args(args)
        .env(sidecar_contract::AUTH_TOKEN_ENV, &launch_token)
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    // Don't pop a console window for the sidecar on Windows (CPE-325).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| format!("spawn {command}: {e}"))?;

    let stdin = child.stdin.take().ok_or("no child stdin")?;
    let stdout = child.stdout.take().ok_or("no child stdout")?;

    // A reader thread turns stdout lines into decoded envelopes on a BOUNDED channel:
    // if the supervisor stops reading, the reader blocks rather than buffering without
    // limit — backpressure toward the child (CPE-297). Each line is itself bounded to
    // MAX_LINE_BYTES (CPE-1471): a child that never emits a newline can't grow the host's
    // memory without limit — on overflow the reader sends one error and stops, which the
    // supervisor already treats as connection loss (capped-backoff restart), same as any
    // other read error.
    let (tx, rx) = mpsc::sync_channel(IPC_CHANNEL_CAPACITY);
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line_buf: Vec<u8> = Vec::new();
        loop {
            line_buf.clear();
            match read_bounded_line(&mut reader, &mut line_buf, MAX_LINE_BYTES) {
                Ok(0) => break, // clean EOF
                Ok(_) => {
                    let text = String::from_utf8_lossy(&line_buf);
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let decoded = Envelope::from_json(trimmed).map_err(|e| format!("decode: {e}"));
                    if tx.send(decoded).is_err() {
                        break; // receiver gone
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                    break;
                }
            }
        }
    });

    Ok(ProcessConnection {
        child,
        stdin,
        rx,
        recv_timeout: Duration::from_secs(5),
        launch_token,
        _reader: reader,
    })
}

impl ProcessConnection {
    /// The per-launch auth token issued to this child (CPE-275) — pass it to
    /// [`handshake`] as the expected token.
    pub fn launch_token(&self) -> &str {
        &self.launch_token
    }
}

impl SidecarChannel for ProcessConnection {
    fn send(&mut self, env: &Envelope) -> Result<(), String> {
        let line = env.to_json().map_err(|e| e.to_string())?;
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("write: {e}"))
    }

    fn recv(&mut self) -> Result<Envelope, String> {
        match self.rx.recv_timeout(self.recv_timeout) {
            Ok(res) => res,
            Err(mpsc::RecvTimeoutError::Timeout) => Err("recv timed out".into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err("sidecar closed the connection".into()),
        }
    }
}

impl Connection for ProcessConnection {
    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn shutdown(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ProcessConnection {
    fn drop(&mut self) {
        // Never leave an orphan.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sidecar_contract::{ContractError, ErrorCode, Hello, Request, Response, CONTRACT_VERSION};
    use std::collections::VecDeque;

    /// In-memory connection scripted with the sidecar's outbound messages.
    struct FakeConn {
        outbox: VecDeque<Envelope>,
        sent: Vec<Envelope>,
        alive: bool,
    }

    impl FakeConn {
        fn new(outbox: Vec<Envelope>) -> Self {
            Self { outbox: outbox.into(), sent: Vec::new(), alive: true }
        }
    }

    impl SidecarChannel for FakeConn {
        fn send(&mut self, env: &Envelope) -> Result<(), String> {
            self.sent.push(env.clone());
            Ok(())
        }
        fn recv(&mut self) -> Result<Envelope, String> {
            self.outbox.pop_front().ok_or_else(|| "empty".into())
        }
    }
    impl Connection for FakeConn {
        fn is_alive(&mut self) -> bool {
            self.alive
        }
        fn shutdown(&mut self) {
            self.alive = false;
        }
    }

    fn hello(major: u16) -> Envelope {
        Envelope::new(
            9,
            Message::Hello(Hello {
                sidecar_id: "fake".into(),
                sidecar_version: "0.1.0".into(),
                contract_version: ContractVersion::new(major, 0),
                capabilities_requested: vec![Capability::Context, Capability::Secrets],
                auth_token: None,
            }),
        )
    }

    #[test]
    fn handshake_grants_the_consented_intersection() {
        let mut conn = FakeConn::new(vec![
            hello(CONTRACT_VERSION.major),
            Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)),
        ]);
        let consented = [Capability::Context].into_iter().collect();
        let out = handshake(&mut conn, CONTRACT_VERSION, &consented, "fake", None).unwrap();
        assert_eq!(out.sidecar_id, "fake");
        // Requested {Context, Secrets} ∩ consented {Context} = {Context}.
        assert_eq!(out.granted, [Capability::Context].into_iter().collect());
        // The Welcome was sent, correlated to the Hello's id.
        assert!(matches!(conn.sent[0].message, Message::Welcome(_)));
        assert_eq!(conn.sent[0].id, 9);
    }

    #[test]
    fn handshake_refuses_an_incompatible_major_and_sends_rejected() {
        let mut conn = FakeConn::new(vec![hello(CONTRACT_VERSION.major + 1)]);
        let err = handshake(&mut conn, CONTRACT_VERSION, &BTreeSet::new(), "fake", None).unwrap_err();
        assert!(matches!(err, HandshakeError::Version(_)));
        // Host told the sidecar why, and did not mount it.
        assert!(matches!(
            conn.sent[0].message,
            Message::Rejected(Rejected { code: RejectCode::IncompatibleVersion, .. })
        ));
    }

    #[test]
    fn handshake_fails_if_ready_never_comes() {
        let mut conn = FakeConn::new(vec![
            hello(CONTRACT_VERSION.major),
            Envelope::new(0, Message::Response(Response { result: Ok(serde_json::Value::Null) })),
        ]);
        let err = handshake(&mut conn, CONTRACT_VERSION, &BTreeSet::new(), "fake", None).unwrap_err();
        assert!(matches!(err, HandshakeError::NoReady(_)));
    }

    fn hello_with_token(token: Option<&str>) -> Envelope {
        Envelope::new(
            9,
            Message::Hello(Hello {
                sidecar_id: "fake".into(),
                sidecar_version: "0.1.0".into(),
                contract_version: CONTRACT_VERSION,
                capabilities_requested: vec![],
                auth_token: token.map(str::to_string),
            }),
        )
    }

    #[test]
    fn handshake_accepts_a_matching_token_and_rejects_a_wrong_one() {
        // Matching token → proceeds to Ready → ok.
        let mut ok = FakeConn::new(vec![
            hello_with_token(Some("secret-token")),
            Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)),
        ]);
        assert!(handshake(&mut ok, CONTRACT_VERSION, &BTreeSet::new(), "fake", Some("secret-token")).is_ok());

        // Wrong token → Untrusted + a Rejected sent.
        let mut bad = FakeConn::new(vec![hello_with_token(Some("wrong"))]);
        let err =
            handshake(&mut bad, CONTRACT_VERSION, &BTreeSet::new(), "fake", Some("secret-token")).unwrap_err();
        assert!(matches!(err, HandshakeError::Untrusted));
        assert!(matches!(
            bad.sent[0].message,
            Message::Rejected(Rejected { code: RejectCode::Untrusted, .. })
        ));

        // Missing token when one is expected → Untrusted.
        let mut missing = FakeConn::new(vec![hello_with_token(None)]);
        assert!(matches!(
            handshake(&mut missing, CONTRACT_VERSION, &BTreeSet::new(), "fake", Some("secret-token"))
                .unwrap_err(),
            HandshakeError::Untrusted
        ));
    }

    /// CPE-1472: a Hello claiming a different `sidecar_id` than the one the host spawned must
    /// be rejected, not trusted at face value — otherwise a sidecar could impersonate another
    /// namespace's identity.
    #[test]
    fn handshake_rejects_a_mismatched_sidecar_id() {
        let mut conn = FakeConn::new(vec![hello(CONTRACT_VERSION.major)]); // hello() declares "fake"
        let err = handshake(&mut conn, CONTRACT_VERSION, &BTreeSet::new(), "not-fake", None).unwrap_err();
        assert!(matches!(err, HandshakeError::Untrusted));
        assert!(matches!(
            conn.sent[0].message,
            Message::Rejected(Rejected { code: RejectCode::Untrusted, .. })
        ));
    }

    #[test]
    fn generated_launch_tokens_are_unique_and_hex() {
        let a = generate_launch_token();
        let b = generate_launch_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn restart_policy_backs_off_then_gives_up() {
        let p = RestartPolicy { max_attempts: 4, base_delay_ms: 100, max_delay_ms: 1000 };
        assert_eq!(p.delay_for(0), Some(100));
        assert_eq!(p.delay_for(1), Some(200));
        assert_eq!(p.delay_for(2), Some(400));
        assert_eq!(p.delay_for(3), Some(800));
        assert_eq!(p.delay_for(4), None, "gives up at the cap");
    }

    #[test]
    fn restart_policy_clamps_to_max_delay() {
        let p = RestartPolicy { max_attempts: 30, base_delay_ms: 1000, max_delay_ms: 5000 };
        assert_eq!(p.delay_for(10), Some(5000));
    }

    // Silence "unused" warnings for the error helper referenced only in E2E tests.
    #[test]
    fn contract_error_helper_is_usable() {
        let _ = ContractError::new(ErrorCode::ToolFailure, "x", false);
        let _ = Request { method: "x".into(), params: serde_json::Value::Null };
    }

    // -----------------------------------------------------------------------
    // CPE-1471: bounded stdout line reader
    // -----------------------------------------------------------------------

    #[test]
    fn bounded_reader_errors_at_the_cap_instead_of_buffering_an_unterminated_line_in_full() {
        // A "line" with no newline anywhere, much bigger than a small test cap: the old
        // `BufRead::lines()` would happily buffer all of it before ever returning. The bounded
        // reader must bail at the cap instead.
        let huge = vec![b'x'; 10_000];
        let mut cursor = std::io::Cursor::new(huge);
        let mut buf = Vec::new();
        let err = read_bounded_line(&mut cursor, &mut buf, 64).unwrap_err();
        assert!(err.contains("64"), "error should mention the cap: {err}");
        assert!(buf.len() <= 64, "must not buffer past the cap, got {} bytes", buf.len());
    }

    #[test]
    fn bounded_reader_reads_a_normal_line_under_the_cap() {
        let mut cursor = std::io::Cursor::new(b"hello world\nnext line\n".to_vec());
        let mut buf = Vec::new();
        let n = read_bounded_line(&mut cursor, &mut buf, 1024).unwrap();
        assert_eq!(n, 11);
        assert_eq!(&buf, b"hello world");
    }

    #[test]
    fn bounded_reader_treats_a_final_unterminated_line_as_eof() {
        let mut cursor = std::io::Cursor::new(b"no newline here".to_vec());
        let mut buf = Vec::new();
        let n = read_bounded_line(&mut cursor, &mut buf, 1024).unwrap();
        assert_eq!(&buf[..n], b"no newline here");
        // The next read is a clean EOF.
        buf.clear();
        assert_eq!(read_bounded_line(&mut cursor, &mut buf, 1024).unwrap(), 0);
    }

    #[test]
    fn bounded_reader_errors_exactly_at_the_real_cap_without_full_allocation() {
        // Sanity-check the actual constant the reader thread uses: a >MAX_LINE_BYTES line with
        // no newline must still error, not silently succeed or hang.
        let huge = vec![b'x'; MAX_LINE_BYTES + 1];
        let mut cursor = std::io::Cursor::new(huge);
        let mut buf = Vec::new();
        let err = read_bounded_line(&mut cursor, &mut buf, MAX_LINE_BYTES).unwrap_err();
        assert!(err.contains("frame too large"));
        assert!(buf.len() <= MAX_LINE_BYTES);
    }
}
