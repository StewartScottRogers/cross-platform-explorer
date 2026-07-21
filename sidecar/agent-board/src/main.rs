//! The Agent Board sidecar process (CPE-851, epic CPE-850). A thin stdio wrapper around the pure protocol
//! in the library (`agent_board`): emit `Hello`, then read JSON-line envelopes; on `Welcome` reach
//! `Ready` and serve the board UI on a loopback port, announcing its URL to the host (`ui:<url>` Status
//! event) — exactly as `sidecar/repos` and the AI Console do. Depends only on `sidecar-contract`.

use std::io::{BufRead, Write};

use agent_board::{board_root, hello, on_message, ui, Reaction};
use sidecar_contract::{Envelope, Event, Lifecycle, Message};

fn emit(out: &mut impl Write, env: &Envelope) {
    let _ = writeln!(out, "{}", env.to_json().expect("serialize"));
    let _ = out.flush();
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    emit(&mut stdout, &hello());

    // Kept alive for the process's lifetime once started (dropping it stops serving the UI).
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
        match on_message(&env) {
            Reaction::Ready => {
                emit(&mut stdout, &Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)));
                if let Ok(server) = ui::serve(board_root()) {
                    emit(
                        &mut stdout,
                        &Envelope::new(0, Message::Event(Event::Status { state: format!("ui:{}", server.url()) })),
                    );
                    _ui_server = Some(server);
                }
            }
            Reaction::Respond(id, resp) => emit(&mut stdout, &Envelope::new(id, Message::Response(resp))),
            Reaction::Shutdown => std::process::exit(0),
            Reaction::Ignore => {}
        }
    }
}
