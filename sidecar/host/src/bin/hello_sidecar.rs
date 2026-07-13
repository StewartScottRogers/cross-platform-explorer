//! # hello_sidecar — the reference sidecar (CPE-273)
//!
//! This is the canonical, copy-me example of a sidecar that speaks the whole
//! `sidecar-contract` protocol and exercises **all four** brokered capabilities
//! (Context, Secrets, Storage, Events) end-to-end — without the AI Console, proving
//! the platform pattern in isolation. It doubles as the fixture the supervisor,
//! broker, and conformance tests spawn as a real process.
//!
//! ## Build your own sidecar from this template
//!
//! A sidecar is just a process that talks JSON-line [`Envelope`]s over stdio. Its
//! source depends on **`sidecar_contract` only** — never on `sidecar_host` internals
//! (that one-way boundary, ADR 0001, is what keeps Mega-Features from entangling the
//! explorer, and it is what the CI delete-test enforces). The shape is always:
//!
//! 1. **Hello** — emit an [`Envelope`] carrying [`Message::Hello`] declaring your id,
//!    version, [`CONTRACT_VERSION`], and the [`Capability`]s you want. You get nothing
//!    you don't ask for, and the host only grants what the user consents to.
//! 2. **Welcome / Ready** — the host replies with [`Message::Welcome`] listing the
//!    capabilities actually *granted*. Emit [`Lifecycle::Ready`] to signal you are up.
//! 3. **Drive capabilities** — call a capability by SENDING a [`Message::Request`]
//!    (e.g. `storage.dir`, `secrets.set`/`secrets.get`, `context.current`) with a
//!    distinct correlation id, and reading the matching [`Message::Response`] back.
//!    Emit [`Message::Event`]s (notify/progress/status) for anything the user should
//!    see. The host enforces the grant on every call; an ungranted call comes back as
//!    a `CapabilityDenied` error response.
//! 4. **Serve requests** — answer inbound [`Message::Request`]s from the host with a
//!    [`Message::Response`] correlated by envelope id; return an error response for a
//!    method you don't know. Exit cleanly on `sidecar.shutdown`.
//!
//! Keep the loop robust: read line-by-line, ignore blank/undecodable lines rather than
//! crashing, and **flush after every write** so the host never blocks waiting on a
//! buffered frame.
//!
//! The fuller SDK — helper crate + a `cargo generate`-style scaffolder so you don't
//! hand-roll the read/write loop — is tracked as CPE-303. Until then, copy this file.

use std::io::{BufRead, Write};

use sidecar_contract::{
    Capability, ContractError, Envelope, ErrorCode, Event, Hello, Level, Lifecycle, Message,
    Request, Response, CONTRACT_VERSION,
};

const SIDECAR_ID: &str = "hello";

/// Write one envelope as a newline-terminated JSON frame, flushing immediately so the
/// host sees it without waiting on a buffer.
fn emit(out: &mut impl Write, env: &Envelope) {
    let _ = writeln!(out, "{}", env.to_json().expect("serialize envelope"));
    let _ = out.flush();
}

/// Emit an error notification and terminate the process non-zero — used when a scripted
/// step fails (an error response, or a value that didn't round-trip).
fn fail(out: &mut impl Write, message: &str) -> ! {
    emit(
        out,
        &Envelope::new(
            0,
            Message::Event(Event::Notify {
                level: Level::Error,
                message: message.into(),
            }),
        ),
    );
    std::process::exit(1);
}

/// Send a capability [`Request`] with correlation id `id`, then block until the matching
/// [`Response`] arrives, returning its JSON result. Any inbound host `Request` that
/// arrives while we wait is answered inline (the host may interleave calls). On an error
/// response, a mismatched/absent correlation, or a closed stream, this does not return —
/// it calls [`fail`].
fn call(
    out: &mut impl Write,
    lines: &mut impl Iterator<Item = std::io::Result<String>>,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    emit(
        out,
        &Envelope::new(
            id,
            Message::Request(Request {
                method: method.into(),
                params,
            }),
        ),
    );

    loop {
        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => fail(out, &format!("stream closed awaiting response to '{method}'")),
        };
        if line.trim().is_empty() {
            continue;
        }
        let env = match Envelope::from_json(line.trim()) {
            Ok(e) => e,
            Err(_) => continue,
        };
        match env.message {
            Message::Response(resp) if env.id == id => match resp.result {
                Ok(value) => return value,
                Err(e) => fail(out, &format!("'{method}' returned error: {}", e.message)),
            },
            // The host can interleave its own requests; answer them the way echo does.
            Message::Request(req) => answer_host_request(out, env.id, &req),
            // Ignore anything else (stray events/signals) while awaiting our reply.
            _ => {}
        }
    }
}

/// Answer an inbound host [`Request`]: `sidecar.shutdown` exits, an unknown method is a
/// structured error, everything else is an ok echo — mirroring `echo_sidecar`.
fn answer_host_request(out: &mut impl Write, env_id: u64, req: &Request) {
    if req.method == "sidecar.shutdown" {
        std::process::exit(0);
    }
    let result = if req.method == "definitely.unknown.method" {
        Err(ContractError::new(ErrorCode::ToolFailure, "unknown method", false))
    } else {
        Ok(serde_json::json!({ "ok": true, "method": req.method }))
    };
    emit(out, &Envelope::new(env_id, Message::Response(Response { result })));
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    // 1. Hello — request every capability this reference exercises.
    emit(
        &mut stdout,
        &Envelope::new(
            0,
            Message::Hello(Hello {
                sidecar_id: SIDECAR_ID.into(),
                sidecar_version: env!("CARGO_PKG_VERSION").into(),
                contract_version: CONTRACT_VERSION,
                capabilities_requested: vec![
                    Capability::Context,
                    Capability::Secrets,
                    Capability::Storage,
                    Capability::Events,
                ],
            }),
        ),
    );

    let mut lines = stdin.lock().lines();

    // 2. Wait for Welcome, capturing what was actually GRANTED, then announce Ready.
    //    (Ignore anything before Welcome.)
    let granted: Vec<Capability> = loop {
        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => return, // stream closed before handshake completed
        };
        if line.trim().is_empty() {
            continue;
        }
        let env = match Envelope::from_json(line.trim()) {
            Ok(e) => e,
            Err(_) => continue,
        };
        match env.message {
            Message::Welcome(w) => {
                emit(&mut stdout, &Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)));
                break w.capabilities_granted;
            }
            Message::Request(req) => answer_host_request(&mut stdout, env.id, &req),
            _ => {}
        }
    };

    // 3. Scripted capability tour — each step runs ONLY for a capability we were
    //    actually granted. A sidecar exercises no authority it wasn't given; a host
    //    that grants nothing (e.g. the conformance kit) sees a passive, echo-like
    //    sidecar and none of these proactive requests. Distinct correlation ids.

    if granted.contains(&Capability::Storage) {
        let storage = call(&mut stdout, &mut lines, 1, "storage.dir", serde_json::Value::Null);
        if storage.get("dir").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            fail(&mut stdout, "storage.dir returned no 'dir'");
        }
    }

    if granted.contains(&Capability::Secrets) {
        // Set a value, then read it back and assert it round-trips.
        let secret_name = "hello-token";
        let secret_value = "s3cr3t-value";
        call(
            &mut stdout,
            &mut lines,
            2,
            "secrets.set",
            serde_json::json!({ "name": secret_name, "value": secret_value }),
        );
        let got = call(
            &mut stdout,
            &mut lines,
            3,
            "secrets.get",
            serde_json::json!({ "name": secret_name }),
        );
        if got.get("value").and_then(|v| v.as_str()) != Some(secret_value) {
            fail(&mut stdout, "secrets round-trip mismatch");
        }
    }

    if granted.contains(&Capability::Context) {
        // Read the explorer's current snapshot.
        call(&mut stdout, &mut lines, 4, "context.current", serde_json::Value::Null);
    }

    if granted.contains(&Capability::Events) {
        // A human-visible notification, then a terminal "done" status the host waits on.
        emit(
            &mut stdout,
            &Envelope::new(
                0,
                Message::Event(Event::Notify {
                    level: Level::Info,
                    message: "hello sidecar exercised context, secrets, storage".into(),
                }),
            ),
        );
        emit(
            &mut stdout,
            &Envelope::new(0, Message::Event(Event::Status { state: "done".into() })),
        );
    }

    // 4. Stay responsive to host requests (e.g. sidecar.shutdown) until the stream ends.
    for line in lines {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let env = match Envelope::from_json(line.trim()) {
            Ok(e) => e,
            Err(_) => continue,
        };
        if let Message::Request(req) = env.message {
            answer_host_request(&mut stdout, env.id, &req);
        }
    }
}
