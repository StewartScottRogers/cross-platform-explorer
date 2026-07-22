//! `cpe-server-ref` — the deployable reference headless Server (CPE-825, epic CPE-810).
//!
//! Wraps the CPE-824 dispatcher and a security chain over a loopback TCP listener, proving the
//! CPE-810 loop is *runnable*, not merely unit-testable. It runs the same [`ServerRuntime`] the
//! tests exercise, so `cargo run -p cpe-net --bin cpe-server-ref` gives a live Server a
//! `Client(Rust)` (or, later, the GUI through the CPE-819 transport seam) can drive.
//!
//! Usage: `cpe-server-ref [ADDR]` (default `127.0.0.1:0` — an OS-assigned port, printed on
//! start). v1 runs the local/passthrough security chain (single-user, trusted loopback); the
//! remote AuthN/AuthZ/transport stacks are config-driven and swap in without touching this
//! binary (CPE-817/818).

use std::net::TcpListener;
use std::sync::Arc;

use cpe_net::ServerRuntime;
use cpe_security::SecurityChain;
use cpe_server::ctx::HeadlessCtx;
use cpe_server::dispatch::Dispatcher;

fn main() -> std::io::Result<()> {
    let addr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:0".to_string());
    let listener = TcpListener::bind(&addr)?;
    let local = listener.local_addr()?;
    println!("cpe-server-ref listening on {local}");

    let base = std::env::temp_dir().join("cpe-server-ref");
    let runtime = Arc::new(
        ServerRuntime::new(
            Dispatcher::with_builtins(),
            SecurityChain::local(),
            Arc::new(HeadlessCtx::new(base)),
        )
        .with_builtin_streams(), // list_dir_stream over the wire (CPE-819)
    );
    runtime.serve(listener)
}
