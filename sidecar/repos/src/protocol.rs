//! The Repositories sidecar's base protocol skeleton (CPE-432).
//!
//! Mirrors the AI Console's CPE-277 skeleton: the opening `Hello` (identity + requested
//! capabilities), and a **pure** `on_message` reaction so the process loop is unit-testable without
//! stdio. The stdio driver in `main.rs` just pumps envelopes through here. Later slices extend the
//! request handling (forge browse/clone/sync) on top.

use sidecar_contract::{Capability, Envelope, Hello, Lifecycle, Message, Response, CONTRACT_VERSION};

/// The sidecar's stable id (matches its manifest / the host's launch config).
pub const SIDECAR_ID: &str = "repos";

/// Capabilities the Repositories sidecar requests at handshake:
/// - [`Capability::Context`] — the open folder (a clone target / the repo you're in),
/// - [`Capability::Secrets`] — forge tokens / SSH keys, per-provider namespace (CPE-439),
/// - [`Capability::Storage`] — its own config + provider/registry cache,
/// - [`Capability::Events`] — surface clone/sync progress + errors,
/// - [`Capability::Network`] — host-brokered, allow-listed forge API egress (CPE-433).
pub const REQUESTED_CAPABILITIES: [Capability; 5] = [
    Capability::Context,
    Capability::Secrets,
    Capability::Storage,
    Capability::Events,
    Capability::Network,
];

/// What the process should do after handling one inbound message. Pure ⇒ unit-testable.
#[derive(Debug, PartialEq)]
pub enum Reaction {
    /// Write this envelope to the host.
    Send(Envelope),
    /// Exit the process with this code.
    Exit(i32),
    /// Do nothing.
    Nothing,
}

/// The opening `Hello`: identity + requested capabilities + the per-launch auth token.
pub fn hello() -> Envelope {
    Envelope::new(
        0,
        Message::Hello(Hello {
            sidecar_id: SIDECAR_ID.into(),
            sidecar_version: env!("CARGO_PKG_VERSION").into(),
            contract_version: CONTRACT_VERSION,
            capabilities_requested: REQUESTED_CAPABILITIES.to_vec(),
            auth_token: std::env::var(sidecar_contract::AUTH_TOKEN_ENV).ok(),
        }),
    )
}

/// The base reaction to one inbound message: reach `Ready` after `Welcome`, exit on a rejection /
/// quit signal / `sidecar.shutdown`, and acknowledge other requests. Forge methods extend the
/// request arm in later slices.
pub fn on_message(env: Envelope) -> Reaction {
    match env.message {
        Message::Welcome(_) => Reaction::Send(Envelope::new(0, Message::Lifecycle(Lifecycle::Ready))),
        Message::Rejected(_) => Reaction::Exit(1),
        Message::Signal(sidecar_contract::HostSignal::WillQuit) => Reaction::Exit(0),
        Message::Request(req) if req.method == "sidecar.shutdown" => Reaction::Exit(0),
        Message::Request(_) => Reaction::Send(Envelope::new(
            env.id,
            Message::Response(Response { result: Ok(serde_json::json!({ "ok": true })) }),
        )),
        _ => Reaction::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sidecar_contract::{HostSignal, Request, Welcome};

    #[test]
    fn hello_declares_identity_and_the_five_capabilities() {
        let Message::Hello(h) = hello().message else { panic!("not a Hello") };
        assert_eq!(h.sidecar_id, "repos");
        assert_eq!(h.contract_version, CONTRACT_VERSION);
        assert_eq!(h.capabilities_requested.len(), 5);
        assert!(h.capabilities_requested.contains(&Capability::Network));
        assert!(h.capabilities_requested.contains(&Capability::Secrets));
    }

    #[test]
    fn welcome_reaches_ready() {
        let welcome = Envelope::new(
            0,
            Message::Welcome(Welcome {
                negotiated_version: CONTRACT_VERSION,
                capabilities_granted: REQUESTED_CAPABILITIES.to_vec(),
            }),
        );
        match on_message(welcome) {
            Reaction::Send(env) => assert!(matches!(env.message, Message::Lifecycle(Lifecycle::Ready))),
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[test]
    fn shutdown_request_and_quit_signal_exit_cleanly() {
        let shutdown = Envelope::new(7, Message::Request(Request { method: "sidecar.shutdown".into(), params: serde_json::Value::Null }));
        assert_eq!(on_message(shutdown), Reaction::Exit(0));
        let quit = Envelope::new(0, Message::Signal(HostSignal::WillQuit));
        assert_eq!(on_message(quit), Reaction::Exit(0));
    }

    #[test]
    fn an_unknown_request_is_acknowledged_not_fatal() {
        let req = Envelope::new(3, Message::Request(Request { method: "forge.something".into(), params: serde_json::Value::Null }));
        match on_message(req) {
            Reaction::Send(env) => {
                assert_eq!(env.id, 3);
                assert!(matches!(env.message, Message::Response(_)));
            }
            other => panic!("expected an ack Response, got {other:?}"),
        }
    }
}
