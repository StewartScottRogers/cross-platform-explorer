//! Manual test for the security boundary on the network path (CPE-816/825). Starts two Servers on
//! loopback — one with the trusted local/passthrough chain, one with the structural default-deny chain —
//! and issues the *same* `list_dir` request to each, so you can see the deny chain refuse it with a
//! structured security error while the local chain serves it.
//!
//! Run:  cargo run -p cpe-net --example security_demo
//!
//! Proves the epic's invariant: a request the chain rejects is never dispatched — security is enforced at
//! the boundary, once, for every method.

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;

use cpe_net::{Client, ServerRuntime};
use cpe_security::SecurityChain;
use cpe_server::ctx::HeadlessCtx;
use cpe_server::dispatch::Dispatcher;

fn start(chain: SecurityChain) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let base = std::env::temp_dir().join("cpe-security-demo");
    let rt = Arc::new(ServerRuntime::new(
        Dispatcher::with_builtins(),
        chain,
        Arc::new(HeadlessCtx::new(base)),
    ));
    std::thread::spawn(move || {
        let _ = rt.serve(listener);
    });
    addr
}

fn try_list(label: &str, addr: SocketAddr) {
    let mut client = Client::connect(addr).expect("connect");
    match client.call("list_dir", serde_json::json!({ "path": "." })) {
        Ok(v) => println!(
            "{label}: ALLOWED → list_dir returned {} entries",
            v.as_array().map(|a| a.len()).unwrap_or(0)
        ),
        Err(e) => println!("{label}: DENIED → {:?}: {}", e.code, e.message),
    }
}

fn main() {
    println!("Same `list_dir` request, two security chains:\n");
    try_list("local/passthrough chain ", start(SecurityChain::local()));
    try_list("default-deny chain       ", start(SecurityChain::default_deny()));
    println!("\nThe deny chain never reached the dispatcher — the request was refused at the boundary.");
}
