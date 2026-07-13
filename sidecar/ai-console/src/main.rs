//! The AI Console sidecar process (CPE-277/271). A thin stdio wrapper around the pure
//! protocol loop in the library: emit `Hello`, then read JSON-line envelopes and act on
//! each. On `Welcome` it also starts its **own UI** server (CPE-271) and announces the
//! loopback URL to the host via a `Status` event (`ui:<url>`), which the host embeds in
//! an iframe pane. Depends only on `sidecar-contract` + this crate's own modules.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use ai_console::agents::AgentRegistry;
use ai_console::console::{route, ws_route, ConsoleState};
use ai_console::{hello, http, on_message, Reaction};
use sidecar_contract::{Envelope, Event, Lifecycle, Message};

fn write_env(out: &mut impl Write, env: &Envelope) {
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

/// Build the launcher's shared state: the agent catalog + the folder sessions default to.
fn console_state() -> Arc<ConsoleState> {
    let registry = AgentRegistry::load_from_dirs(&[agents_dir()]);
    let cwd = std::env::var("CPE_AICONSOLE_CWD")
        .ok()
        .or_else(|| std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_default();
    Arc::new(ConsoleState::new(registry, cwd))
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    write_env(&mut stdout, &hello());

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

        // On Welcome: reach Ready, start the UI server, and announce its URL so the host
        // can mount it (CPE-271). Handled here (not in the pure `on_message`) because it
        // has side effects.
        if matches!(env.message, Message::Welcome(_)) {
            write_env(&mut stdout, &Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)));
            if let Ok(server) = http::serve(console_state(), route, ws_route) {
                write_env(
                    &mut stdout,
                    &Envelope::new(0, Message::Event(Event::Status { state: format!("ui:{}", server.url()) })),
                );
                _ui_server = Some(server);
            }
            continue;
        }

        match on_message(env) {
            Reaction::Send(reply) => write_env(&mut stdout, &reply),
            Reaction::Exit(code) => std::process::exit(code),
            Reaction::Nothing => {}
        }
    }
}
