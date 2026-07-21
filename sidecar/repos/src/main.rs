//! The Repositories sidecar process (CPE-432). A thin stdio wrapper around the pure protocol loop
//! in the library: emit `Hello`, then read JSON-line envelopes and act on each. On `Welcome` it
//! reaches `Ready`, then serves its **own** loopback UI and announces the URL to the host
//! (`ui:<url>` Status event, CPE-432 AC2) — exactly as the Agent Deck's `main.rs` does. Host-brokered
//! forge egress (CPE-433) and the browse/clone/sync request methods layer onto the `Request` arm in
//! later slices. Depends only on `sidecar-contract` + this crate's own modules.

use std::io::{BufRead, Write};

use repos::ui;
use repos::{hello, on_message, Reaction};
use sidecar_contract::{Envelope, Event, Lifecycle, Message};

fn write_env(env: &Envelope) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", env.to_json().expect("serialize"));
    let _ = out.flush();
}

fn main() {
    let stdin = std::io::stdin();
    write_env(&hello());

    // Kept alive for the process's lifetime once started (dropping it would stop serving the UI).
    let mut _ui_server: Option<ui::UiServer> = None;

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

        // On Welcome: reach Ready, then start the UI server and announce its loopback URL so the host
        // can embed it. Both are side-effecting, so they live here rather than in pure `on_message`.
        if matches!(env.message, Message::Welcome(_)) {
            write_env(&Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)));
            if let Ok(server) = ui::serve(ui::placeholder_ui()) {
                write_env(&Envelope::new(
                    0,
                    Message::Event(Event::Status { state: format!("ui:{}", server.url()) }),
                ));
                _ui_server = Some(server);
            }
            continue;
        }

        match on_message(env) {
            Reaction::Send(reply) => write_env(&reply),
            Reaction::Exit(code) => std::process::exit(code),
            Reaction::Nothing => {}
        }
    }
}
