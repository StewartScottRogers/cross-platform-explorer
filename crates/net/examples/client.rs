//! Manual test for the network transport loop (CPE-825, `cpe-net`): a real `Client(Rust)` that drives a
//! Server over a TCP socket, proving `GUI → Client(Rust) → Server(Rust)` end-to-end.
//!
//! Two terminals:
//!   1)  cargo run -p cpe-net --bin cpe-server-ref -- 127.0.0.1:9876
//!   2)  cargo run -p cpe-net --example client -- 127.0.0.1:9876 <dir-to-list>
//!
//! The client handshakes (version negotiation), then calls `list_dir` over the wire and prints the
//! entries the Server returned — the same request/response the local in-process path uses.

use cpe_net::Client;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: client <addr> [dir-to-list]");
        eprintln!("  start the server first:");
        eprintln!("  cargo run -p cpe-net --bin cpe-server-ref -- 127.0.0.1:9876");
        std::process::exit(2);
    }
    let addr = args[1].as_str();
    let dir = args.get(2).cloned().unwrap_or_else(|| ".".to_string());

    let mut client = match Client::connect(addr) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("connect failed: {e}");
            std::process::exit(1);
        }
    };
    println!("connected to {addr}; negotiated contract version {}", client.negotiated_version());

    match client.call("list_dir", serde_json::json!({ "path": dir })) {
        Ok(value) => {
            let entries = value.as_array().cloned().unwrap_or_default();
            println!("list_dir({dir:?}) over the wire → {} entries:", entries.len());
            for e in entries.iter().take(40) {
                let kind = if e["is_dir"].as_bool().unwrap_or(false) { "[dir] " } else { "[file]" };
                println!("  {kind} {}", e["name"].as_str().unwrap_or("?"));
            }
        }
        Err(e) => eprintln!("list_dir failed: {:?} — {}", e.code, e.message),
    }
}
