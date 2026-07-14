//! The AI Console sidecar process (CPE-277/271). A thin stdio wrapper around the pure
//! protocol loop in the library: emit `Hello`, then read JSON-line envelopes and act on
//! each. On `Welcome` it also starts its **own UI** server (CPE-271) and announces the
//! loopback URL to the host via a `Status` event (`ui:<url>`), which the host embeds in
//! an iframe pane. Depends only on `sidecar-contract` + this crate's own modules.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ai_console::agents::AgentRegistry;
use ai_console::broker_client::{BrokerClient, BrokerPresets, BrokerSecrets, SharedWriter};
use ai_console::console::{route, ws_route, ConsoleState};
use ai_console::presets::PresetsBackend;
use ai_console::vault::SecretAccess;
use ai_console::{hello, http, on_message, Reaction};
use sidecar_contract::{Envelope, Event, Lifecycle, Message};

fn write_env(writer: &SharedWriter, env: &Envelope) {
    let mut out = writer.lock().unwrap();
    let _ = writeln!(out, "{}", env.to_json().expect("serialize"));
    let _ = out.flush();
}

/// Locate the bundled agent catalog: an explicit override, then `agents/` next to the
/// executable (how it ships), then the dev-tree copy.
fn agents_dir() -> PathBuf {
    if let Ok(p) = std::env::var("CPE_AICONSOLE_AGENTS") {
        let pb = PathBuf::from(p);
        if pb.is_dir() {
            return pb;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("agents");
            if p.is_dir() {
                return p;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents")
}

/// Build the launcher's shared state: the agent catalog, the default folder, and the
/// secrets backend (the host keychain broker) so provider keys resolve at launch.
fn console_state(
    secrets: Arc<dyn SecretAccess + Send + Sync>,
    presets: Arc<dyn PresetsBackend>,
) -> Arc<ConsoleState> {
    let registry = AgentRegistry::load_from_dirs(&[agents_dir()]);
    let cwd = std::env::var("CPE_AICONSOLE_CWD")
        .ok()
        .or_else(|| std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_default();
    Arc::new(ConsoleState::with_backends(registry, cwd, secrets, presets))
}

fn main() {
    let stdin = std::io::stdin();
    // A single shared writer so the main loop and the broker client never interleave
    // partial lines on stdout (CPE-344).
    let writer: SharedWriter = Arc::new(Mutex::new(Box::new(std::io::stdout())));
    // Outbound capability client (secrets.*). The loopback HTTP handlers call it; the
    // main loop below routes the host's responses back to it.
    let broker = Arc::new(BrokerClient::new(writer.clone()));

    write_env(&writer, &hello());

    // Kept alive for the process lifetime once the handshake completes.
    let mut _ui_server: Option<http::UiServer> = None;

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

        // A response to one of our outbound capability requests: hand it to the waiter.
        if let Message::Response(resp) = &env.message {
            broker.deliver(env.id, resp.clone());
            continue;
        }

        // On Welcome: reach Ready, start the UI server, and announce its URL so the host
        // can mount it (CPE-271). Handled here (not in the pure `on_message`) because it
        // has side effects. The console gets a keychain-backed secrets store (CPE-344).
        if matches!(env.message, Message::Welcome(_)) {
            write_env(&writer, &Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)));
            let secrets = Arc::new(BrokerSecrets::new(broker.clone()));
            let presets = Arc::new(BrokerPresets::new(broker.clone()));
            if let Ok(server) = http::serve(console_state(secrets, presets), route, ws_route) {
                write_env(
                    &writer,
                    &Envelope::new(0, Message::Event(Event::Status { state: format!("ui:{}", server.url()) })),
                );
                _ui_server = Some(server);
            }
            continue;
        }

        match on_message(env) {
            Reaction::Send(reply) => write_env(&writer, &reply),
            Reaction::Exit(code) => std::process::exit(code),
            Reaction::Nothing => {}
        }
    }
}
