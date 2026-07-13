//! Minimal contract-speaking sidecar — a test fixture for the supervisor (CPE-265)
//! and conformance kit (CPE-301). It is NOT the full hello sidecar (CPE-273), which
//! adds every capability and a UI; this one exists only to exercise the real process
//! boundary end-to-end.
//!
//! Protocol: emit `Hello`; on `Welcome` emit `Lifecycle::Ready`; answer any `Request`
//! with a `Response` (error for `definitely.unknown.method`), and exit on
//! `sidecar.shutdown`.

use std::io::{BufRead, Write};

use sidecar_contract::{
    Capability, ContractError, Envelope, ErrorCode, Hello, Lifecycle, Message, Response,
    CONTRACT_VERSION,
};

fn emit(out: &mut impl Write, env: &Envelope) {
    let _ = writeln!(out, "{}", env.to_json().expect("serialize"));
    let _ = out.flush();
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    emit(
        &mut stdout,
        &Envelope::new(
            0,
            Message::Hello(Hello {
                sidecar_id: "echo".into(),
                sidecar_version: env!("CARGO_PKG_VERSION").into(),
                contract_version: CONTRACT_VERSION,
                capabilities_requested: vec![Capability::Context],
                auth_token: std::env::var(sidecar_contract::AUTH_TOKEN_ENV).ok(),
            }),
        ),
    );

    for line in stdin.lock().lines() {
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
        match env.message {
            Message::Welcome(_) => {
                emit(&mut stdout, &Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)));
            }
            Message::Request(req) => {
                if req.method == "sidecar.shutdown" {
                    std::process::exit(0);
                }
                let result = if req.method == "definitely.unknown.method" {
                    Err(ContractError::new(ErrorCode::ToolFailure, "unknown method", false))
                } else {
                    Ok(serde_json::json!({ "ok": true, "method": req.method }))
                };
                emit(
                    &mut stdout,
                    &Envelope::new(env.id, Message::Response(Response { result })),
                );
            }
            _ => {}
        }
    }
}
