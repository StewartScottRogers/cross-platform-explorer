//! Contract conformance kit (CPE-301).
//!
//! "Add more sidecars easily" only holds if a candidate sidecar can *prove* it speaks
//! the contract. This is a reusable, transport-agnostic battery that drives a sidecar
//! through the handshake and core protocol behaviours and reports pass/fail. It talks
//! to the sidecar through the [`SidecarChannel`] trait, so it runs against an
//! in-memory mock (unit tests) or a real child process (wired by the supervisor,
//! CPE-265). The battery grows as the contract grows.

use sidecar_contract::{
    negotiate, ContractVersion, Envelope, Lifecycle, Message, Request, Welcome,
};

/// A bidirectional channel to the sidecar under test. `recv` returns the next
/// envelope the sidecar emitted, or an error on close/timeout.
pub trait SidecarChannel {
    fn send(&mut self, env: &Envelope) -> Result<(), String>;
    fn recv(&mut self) -> Result<Envelope, String>;
}

/// The outcome of one conformance check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

impl CheckResult {
    fn pass(name: &'static str) -> Self {
        Self { name, passed: true, detail: String::new() }
    }
    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self { name, passed: false, detail: detail.into() }
    }
}

/// A full conformance report.
#[derive(Debug, Clone)]
pub struct Report {
    pub checks: Vec<CheckResult>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }
    pub fn failures(&self) -> impl Iterator<Item = &CheckResult> {
        self.checks.iter().filter(|c| !c.passed)
    }
}

/// Drive the sidecar through the battery, presenting `host_version` as the host's
/// contract version. Stops early only if the handshake can't even begin (nothing
/// further is meaningful); otherwise records every check.
pub fn run_conformance(channel: &mut dyn SidecarChannel, host_version: ContractVersion) -> Report {
    let mut checks = Vec::new();

    // 1. The sidecar opens with a well-formed Hello.
    let hello = match channel.recv() {
        Ok(env) => env,
        Err(e) => {
            checks.push(CheckResult::fail("hello_received", format!("no Hello: {e}")));
            return Report { checks };
        }
    };
    let sidecar_version = match &hello.message {
        Message::Hello(h) if !h.sidecar_id.trim().is_empty() => {
            checks.push(CheckResult::pass("hello_well_formed"));
            h.contract_version
        }
        Message::Hello(_) => {
            checks.push(CheckResult::fail("hello_well_formed", "Hello has empty sidecar_id"));
            return Report { checks };
        }
        other => {
            checks.push(CheckResult::fail(
                "hello_well_formed",
                format!("first message was not Hello: {other:?}"),
            ));
            return Report { checks };
        }
    };

    // 2. Envelope schema version is set.
    checks.push(if hello.schema_version == 0 {
        CheckResult::fail("envelope_schema_version", "schema_version is 0")
    } else {
        CheckResult::pass("envelope_schema_version")
    });

    // 3. Version is negotiable (same major).
    match negotiate(host_version, sidecar_version) {
        Ok(agreed) => {
            checks.push(CheckResult::pass("version_negotiable"));
            // Host accepts with a Welcome carrying the negotiated version.
            let welcome = Envelope::new(
                0,
                Message::Welcome(Welcome {
                    negotiated_version: agreed,
                    capabilities_granted: Vec::new(),
                }),
            );
            if let Err(e) = channel.send(&welcome) {
                checks.push(CheckResult::fail("welcome_sent", e));
                return Report { checks };
            }
        }
        Err(e) => {
            checks.push(CheckResult::fail(
                "version_negotiable",
                format!("incompatible: {e:?}"),
            ));
            return Report { checks };
        }
    }

    // 4. The sidecar reports Ready after the Welcome.
    match channel.recv() {
        Ok(env) if matches!(env.message, Message::Lifecycle(Lifecycle::Ready)) => {
            checks.push(CheckResult::pass("reaches_ready"));
        }
        Ok(env) => checks.push(CheckResult::fail(
            "reaches_ready",
            format!("expected Lifecycle::Ready, got {:?}", env.message),
        )),
        Err(e) => checks.push(CheckResult::fail("reaches_ready", e)),
    }

    // 5. Requests are answered, correlated by envelope id.
    checks.push(request_is_correlated(channel, 101, "conformance.echo"));

    // 6. An unknown method yields an error Response, not silence or a crash.
    checks.push(unknown_method_errors(channel, 102));

    Report { checks }
}

fn request_is_correlated(
    channel: &mut dyn SidecarChannel,
    id: u64,
    method: &str,
) -> CheckResult {
    let req = Envelope::new(
        id,
        Message::Request(Request { method: method.into(), params: serde_json::Value::Null }),
    );
    if let Err(e) = channel.send(&req) {
        return CheckResult::fail("response_correlated", e);
    }
    match channel.recv() {
        Ok(env) if env.id == id && matches!(env.message, Message::Response(_) | Message::Error(_)) => {
            CheckResult::pass("response_correlated")
        }
        Ok(env) => CheckResult::fail(
            "response_correlated",
            format!("expected Response with id {id}, got id {} / {:?}", env.id, env.message),
        ),
        Err(e) => CheckResult::fail("response_correlated", e),
    }
}

fn unknown_method_errors(channel: &mut dyn SidecarChannel, id: u64) -> CheckResult {
    let req = Envelope::new(
        id,
        Message::Request(Request {
            method: "definitely.unknown.method".into(),
            params: serde_json::Value::Null,
        }),
    );
    if let Err(e) = channel.send(&req) {
        return CheckResult::fail("unknown_method_errors", e);
    }
    match channel.recv() {
        Ok(env) => match env.message {
            Message::Error(_) => CheckResult::pass("unknown_method_errors"),
            Message::Response(r) if r.result.is_err() => CheckResult::pass("unknown_method_errors"),
            other => CheckResult::fail(
                "unknown_method_errors",
                format!("expected an error, got {other:?}"),
            ),
        },
        Err(e) => CheckResult::fail("unknown_method_errors", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sidecar_contract::{
        Capability, ContractError, ErrorCode, Hello, Response, CONTRACT_VERSION,
    };
    use std::collections::VecDeque;

    /// An in-memory sidecar that behaves correctly, unless a fault is injected.
    struct MockSidecar {
        outbox: VecDeque<Envelope>,
        wrong_correlation: bool,
        never_ready: bool,
    }

    impl MockSidecar {
        fn good() -> Self {
            let mut s = Self { outbox: VecDeque::new(), wrong_correlation: false, never_ready: false };
            s.outbox.push_back(Envelope::new(
                0,
                Message::Hello(Hello {
                    sidecar_id: "mock".into(),
                    sidecar_version: "0.1.0".into(),
                    contract_version: CONTRACT_VERSION,
                    capabilities_requested: vec![Capability::Context],
                }),
            ));
            s
        }
    }

    impl SidecarChannel for MockSidecar {
        fn send(&mut self, env: &Envelope) -> Result<(), String> {
            // React to what the host sent, enqueueing the sidecar's reply.
            match &env.message {
                Message::Welcome(_) => {
                    if !self.never_ready {
                        self.outbox
                            .push_back(Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)));
                    }
                }
                Message::Request(req) => {
                    let reply_id = if self.wrong_correlation { env.id + 1 } else { env.id };
                    let msg = if req.method == "definitely.unknown.method" {
                        Message::Response(Response {
                            result: Err(ContractError::new(ErrorCode::ToolFailure, "unknown", false)),
                        })
                    } else {
                        Message::Response(Response { result: Ok(serde_json::json!({ "ok": true })) })
                    };
                    self.outbox.push_back(Envelope::new(reply_id, msg));
                }
                _ => {}
            }
            Ok(())
        }
        fn recv(&mut self) -> Result<Envelope, String> {
            self.outbox.pop_front().ok_or_else(|| "no message".to_string())
        }
    }

    #[test]
    fn a_well_behaved_sidecar_passes_every_check() {
        let mut s = MockSidecar::good();
        let report = run_conformance(&mut s, CONTRACT_VERSION);
        assert!(report.passed(), "failures: {:?}", report.failures().collect::<Vec<_>>());
        assert_eq!(report.checks.len(), 6);
    }

    #[test]
    fn the_kit_catches_a_correlation_bug() {
        let mut s = MockSidecar::good();
        s.wrong_correlation = true;
        let report = run_conformance(&mut s, CONTRACT_VERSION);
        assert!(!report.passed());
        assert!(report.failures().any(|c| c.name == "response_correlated"));
    }

    #[test]
    fn the_kit_catches_a_missing_ready() {
        let mut s = MockSidecar::good();
        s.never_ready = true;
        let report = run_conformance(&mut s, CONTRACT_VERSION);
        assert!(!report.passed());
        assert!(report.failures().any(|c| c.name == "reaches_ready"));
    }

    #[test]
    fn a_major_incompatible_sidecar_fails_negotiation() {
        // Present the host as major 2 while the mock speaks major 1.
        let mut s = MockSidecar::good();
        let report = run_conformance(&mut s, ContractVersion::new(2, 0));
        assert!(!report.passed());
        assert!(report.failures().any(|c| c.name == "version_negotiable"));
    }
}
