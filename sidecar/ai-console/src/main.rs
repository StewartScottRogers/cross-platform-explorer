//! The AI Console sidecar process (CPE-277). A thin stdio wrapper around the pure
//! protocol loop in the library: emit `Hello`, then read JSON-line envelopes and act
//! on each per [`ai_console::on_message`]. Depends only on `sidecar-contract`.

use std::io::{BufRead, Write};

use ai_console::{hello, on_message, Reaction};
use sidecar_contract::Envelope;

fn write_env(out: &mut impl Write, env: &Envelope) {
    let _ = writeln!(out, "{}", env.to_json().expect("serialize"));
    let _ = out.flush();
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    write_env(&mut stdout, &hello());

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
        match on_message(env) {
            Reaction::Send(reply) => write_env(&mut stdout, &reply),
            Reaction::Exit(code) => std::process::exit(code),
            Reaction::Nothing => {}
        }
    }
}
