//! # cpe-net
//!
//! The **headless network transport loop** for the Cross-Platform Explorer (epic CPE-810,
//! ticket CPE-825): it composes the three standalone crates into a runnable client/server
//! over a socket, closing the remote half of the epic in Rust with no frontend and no codegen:
//!
//! ```text
//! Client(Rust) ──(TCP, CPE-811 envelope)──► Server runtime ──► Dispatcher (CPE-824) ──► domain
//!                                                     ▲
//!                                          SecurityChain (CPE-816) guards every request
//! ```
//!
//! - [`wire`] frames the CPE-811 [`Envelope`](cpe_contract::Envelope) over any `Read`/`Write`.
//! - [`ServerRuntime`] runs the handshake + request loop, enforcing the security chain at the
//!   boundary and dispatching only already-authorized requests.
//! - [`Client`] is the Rust proxy: connect, handshake, `call`.
//!
//! This crate is the composition layer *above* the pure Server, so `cpe-server` never gains a
//! transport or security dependency (the one-way boundary the epic establishes). std-only
//! sockets, thread-per-connection — no async runtime, no heavy deps, so the plain explorer's
//! local path is untouched.

pub mod client;
pub mod server;
pub mod wire;

pub use client::{Client, ConnectError};
pub use server::ServerRuntime;

/// The wire contract, re-exported so a consumer reaches it through the network layer.
pub use cpe_contract as contract;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{SocketAddr, TcpListener};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    use cpe_contract::{ContractVersion, ErrorCode, RejectCode, Request, CONTRACT_VERSION};
    use cpe_security::SecurityChain;
    use cpe_server::ctx::HeadlessCtx;
    use cpe_server::dispatch::Dispatcher;

    /// A unique scratch directory for a test, cleaned up by the OS temp reaper.
    fn scratch(tag: &str) -> std::path::PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-net-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Start a Server on an ephemeral loopback port with the given security chain, and return
    /// its address. The Server runs on a detached thread for the life of the test process.
    fn start_server(chain: SecurityChain) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = scratch("srvbase");
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

    #[test]
    fn loopback_browse_returns_entries() {
        let dir = scratch("browse");
        std::fs::write(dir.join("a.txt"), b"hi").unwrap();

        let addr = start_server(SecurityChain::local());
        let mut client = Client::connect(addr).unwrap();
        let val = client
            .call("list_dir", serde_json::json!({ "path": dir.to_string_lossy() }))
            .expect("remote list_dir should succeed");

        let names: Vec<String> = val
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "a.txt"), "got {names:?}");
    }

    #[test]
    fn two_calls_reuse_one_connection() {
        let dir = scratch("twocalls");
        std::fs::write(dir.join("x.txt"), b"1").unwrap();
        let addr = start_server(SecurityChain::local());
        let mut client = Client::connect(addr).unwrap();

        let path = serde_json::json!({ "path": dir.to_string_lossy() });
        let first = client.call("list_dir", path.clone()).unwrap();
        let second = client.call("list_dir", path).unwrap();
        assert_eq!(first, second, "repeated calls on one connection must be stable");
    }

    #[test]
    fn default_deny_chain_blocks_and_never_dispatches() {
        let dir = scratch("deny");
        std::fs::write(dir.join("secret.txt"), b"x").unwrap();

        // The very same call that succeeds under the local chain (above) must be refused when
        // the boundary is a default-deny chain — proving the request never reached dispatch.
        let addr = start_server(SecurityChain::default_deny());
        let mut client = Client::connect(addr).unwrap();
        let err = client
            .call("list_dir", serde_json::json!({ "path": dir.to_string_lossy() }))
            .expect_err("default-deny must refuse the request");
        assert!(
            matches!(err.code, ErrorCode::Unauthenticated | ErrorCode::Unauthorized),
            "expected a security error, got {:?}: {}",
            err.code,
            err.message
        );
    }

    #[test]
    fn mismatched_major_is_rejected_cleanly() {
        let addr = start_server(SecurityChain::local());
        let incompatible = ContractVersion::new(CONTRACT_VERSION.major + 1, 0);
        match Client::connect_as(addr, incompatible, None) {
            Err(ConnectError::Rejected(r)) => assert_eq!(r.code, RejectCode::IncompatibleVersion),
            Err(other) => panic!("expected a version rejection, got a different error: {other}"),
            Ok(_) => panic!("an incompatible major must not connect"),
        }
    }

    #[test]
    fn compatible_client_negotiates_and_calls() {
        let addr = start_server(SecurityChain::local());
        // Same major as the server; negotiation yields the lower minor.
        let client =
            Client::connect_as(addr, ContractVersion::new(CONTRACT_VERSION.major, 0), None).unwrap();
        assert_eq!(client.negotiated_version().major, CONTRACT_VERSION.major);
        assert!(client.session().is_local(), "v1 handshake establishes the local session");
    }

    #[test]
    fn unknown_method_reports_not_found_over_the_wire() {
        let addr = start_server(SecurityChain::local());
        let mut client = Client::connect(addr).unwrap();
        let err = client
            .call("does_not_exist", serde_json::json!({}))
            .expect_err("unknown method must error");
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    /// Local-fast guard (CPE-810 tiebreaker): the remote machinery must not tax the in-process
    /// path. Measured in-run and compared relatively (never an absolute wall-clock budget), so
    /// it is stable across the 3-OS CI matrix regardless of how loaded a runner is: the
    /// loopback path does everything the in-process path does *plus* the network round-trip, so
    /// it is structurally slower.
    #[test]
    fn in_process_dispatch_is_not_taxed_by_the_remote_path() {
        let dir = scratch("bench");
        std::fs::write(dir.join("a.txt"), b"hi").unwrap();
        let params = serde_json::json!({ "path": dir.to_string_lossy() });
        const N: usize = 200;

        // In-process: direct dispatch, no socket — the local fast path.
        let ctx = HeadlessCtx::new(scratch("benchbase"));
        let dispatcher = Dispatcher::with_builtins();
        let t0 = Instant::now();
        for _ in 0..N {
            let resp = dispatcher.dispatch(
                &ctx,
                Request {
                    method: "list_dir".to_string(),
                    params: params.clone(),
                },
            );
            assert!(resp.result.is_ok());
        }
        let in_process = t0.elapsed();

        // Over loopback: the identical calls through the client/server.
        let addr = start_server(SecurityChain::local());
        let mut client = Client::connect(addr).unwrap();
        let t1 = Instant::now();
        for _ in 0..N {
            client.call("list_dir", params.clone()).unwrap();
        }
        let loopback = t1.elapsed();

        println!("local-fast guard: in_process={in_process:?} loopback={loopback:?} (N={N})");
        assert!(
            in_process < loopback,
            "the in-process path must stay faster than the remote path: in_process={in_process:?} loopback={loopback:?}"
        );
    }
}
