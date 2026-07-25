//! Diagnostic: run the exact flow `sidecar_start_ai_console` uses against the real
//! ai-console binary. Run with:
//!   CPE_AICONSOLE_BIN=<path> cargo test --test ai_console_flow -- --ignored --nocapture

use std::collections::BTreeSet;

use sidecar_contract::{Capability, Event, Message, CONTRACT_VERSION};
use sidecar_host::conformance::SidecarChannel;
use sidecar_host::supervisor::{handshake, spawn_process};

#[test]
#[ignore = "needs CPE_AICONSOLE_BIN set to the built ai-console binary"]
fn ai_console_full_flow() {
    let bin = std::env::var("CPE_AICONSOLE_BIN").expect("set CPE_AICONSOLE_BIN");
    eprintln!("bin = {bin}");

    let mut conn = spawn_process(&bin, &[]).expect("spawn_process failed");
    let token = conn.launch_token().to_string();
    eprintln!("spawned; token = {token}");

    let consented: BTreeSet<Capability> =
        [Capability::Context, Capability::Secrets, Capability::Storage, Capability::Events]
            .into_iter()
            .collect();
    let outcome = handshake(&mut conn, CONTRACT_VERSION, &consented, Some(&token));
    eprintln!("handshake = {outcome:?}");
    let _outcome = outcome.expect("handshake failed");

    let mut url = None;
    for i in 0..20 {
        match conn.recv() {
            Ok(env) => {
                eprintln!("frame[{i}] = {:?}", env.message);
                if let Message::Event(Event::Status { state }) = env.message {
                    if let Some(u) = state.strip_prefix("ui:") {
                        url = Some(u.to_string());
                        break;
                    }
                }
            }
            Err(e) => {
                eprintln!("recv[{i}] error: {e}");
                break;
            }
        }
    }
    eprintln!("URL = {url:?}");
    assert!(url.is_some(), "no ui: announcement received");
}
