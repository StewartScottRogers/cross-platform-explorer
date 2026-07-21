//! The Agent Board sidecar — pure protocol + its served UI (CPE-851, epic CPE-850).
//!
//! An out-of-process sidecar on the ADR-0001 platform, mirroring `sidecar/repos` and
//! `sidecar/ai-console`: it depends **only** on [`sidecar_contract`] (never the host or the explorer),
//! handshakes over stdio, and serves its **own** loopback UI (the Kanban board over `Tickets/`, real data
//! in CPE-852) which the host frames. The stdio side effects live in `main`; the decisions here are pure
//! and unit-tested.

/// The sidecar's own served UI (a dependency-free loopback HTTP server).
pub mod ui;

use sidecar_contract::{Capability, Envelope, Hello, Message, Response, CONTRACT_VERSION};

/// This sidecar's stable id, matching `sidecar.json` and the `Hello`.
pub const SIDECAR_ID: &str = "agent-board";

/// The opening `Hello` this sidecar announces: its id/version, the contract version it speaks, and the
/// capabilities it requests — `context`, to learn the project root whose `Tickets/` it reads (CPE-852).
pub fn hello() -> Envelope {
    Envelope::new(
        0,
        Message::Hello(Hello {
            sidecar_id: SIDECAR_ID.into(),
            sidecar_version: env!("CARGO_PKG_VERSION").into(),
            contract_version: CONTRACT_VERSION,
            capabilities_requested: vec![Capability::Context],
            auth_token: std::env::var(sidecar_contract::AUTH_TOKEN_ENV).ok(),
        }),
    )
}

/// What the stdio loop should do for one incoming envelope. Pure so it can be unit-tested; the side
/// effects (start the UI, announce its URL, exit) happen in `main`.
pub enum Reaction {
    /// Handshake accepted — reach `Ready`, then start the UI server and announce its URL.
    Ready,
    /// Reply to a request, correlated by the given envelope id.
    Respond(u64, Response),
    /// Shut the process down.
    Shutdown,
    /// Nothing to do.
    Ignore,
}

/// Pure protocol decision for one incoming envelope: `Welcome` → Ready; `sidecar.shutdown` → Shutdown;
/// any other request → an `{ ok: true }` acknowledgement (real board methods land in CPE-852).
pub fn on_message(env: &Envelope) -> Reaction {
    match &env.message {
        Message::Welcome(_) => Reaction::Ready,
        Message::Request(req) => {
            if req.method == "sidecar.shutdown" {
                Reaction::Shutdown
            } else {
                Reaction::Respond(env.id, Response { result: Ok(serde_json::json!({ "ok": true })) })
            }
        }
        _ => Reaction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sidecar_contract::{Lifecycle, Request, Welcome};

    #[test]
    fn hello_announces_the_agent_board_id_and_context_capability() {
        let env = hello();
        match env.message {
            Message::Hello(h) => {
                assert_eq!(h.sidecar_id, "agent-board");
                assert_eq!(h.contract_version, CONTRACT_VERSION);
                assert!(h.capabilities_requested.contains(&Capability::Context));
            }
            _ => panic!("hello() must be a Hello"),
        }
    }

    #[test]
    fn welcome_reaches_ready() {
        let env = Envelope::new(0, Message::Welcome(Welcome {
            negotiated_version: CONTRACT_VERSION,
            capabilities_granted: vec![Capability::Context],
        }));
        assert!(matches!(on_message(&env), Reaction::Ready));
    }

    #[test]
    fn shutdown_request_shuts_down_others_are_acked() {
        let shut = Envelope::new(1, Message::Request(Request { method: "sidecar.shutdown".into(), params: serde_json::json!({}) }));
        assert!(matches!(on_message(&shut), Reaction::Shutdown));

        let other = Envelope::new(7, Message::Request(Request { method: "board.ping".into(), params: serde_json::json!({}) }));
        match on_message(&other) {
            Reaction::Respond(id, resp) => {
                assert_eq!(id, 7);
                assert!(resp.result.is_ok());
            }
            _ => panic!("a non-shutdown request should be acknowledged"),
        }
    }

    #[test]
    fn unrelated_messages_are_ignored() {
        let env = Envelope::new(0, Message::Lifecycle(Lifecycle::Ready));
        assert!(matches!(on_message(&env), Reaction::Ignore));
    }
}
