//! The Repositories sidecar process (CPE-432). A thin stdio wrapper around the pure protocol loop
//! in the library: emit `Hello`, then read JSON-line envelopes and act on each. On `Welcome` it
//! reaches `Ready`. Depends only on `sidecar-contract` + this crate's own modules.
//!
//! This is the **base skeleton**: it handshakes and answers lifecycle. The UI server + its
//! `ui:<url>` announce (CPE-435), host-brokered forge egress (CPE-433), and the browse/clone/sync
//! request methods layer onto the `Welcome`/`Request` arms in later slices, exactly as the AI
//! Console's `main.rs` grew from the same skeleton.

use std::io::{BufRead, Write};

use repos::{hello, on_message, Reaction};
use sidecar_contract::{Envelope, Lifecycle, Message};

fn write_env(env: &Envelope) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", env.to_json().expect("serialize"));
    let _ = out.flush();
}

fn main() {
    let stdin = std::io::stdin();
    write_env(&hello());

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

        // On Welcome: reach Ready. Later slices start the UI server here and announce its URL
        // (side-effecting, so kept out of the pure `on_message`).
        if matches!(env.message, Message::Welcome(_)) {
            write_env(&Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)));
            continue;
        }

        match on_message(env) {
            Reaction::Send(reply) => write_env(&reply),
            Reaction::Exit(code) => std::process::exit(code),
            Reaction::Nothing => {}
        }
    }
}
