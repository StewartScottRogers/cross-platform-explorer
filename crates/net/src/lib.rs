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
    use std::time::{Duration, Instant};

    use cpe_contract::{ContractVersion, ErrorCode, Principal, RejectCode, Request, CONTRACT_VERSION};
    use cpe_security::authn::ApiTokenAuthenticator;
    use cpe_security::authz::{CapabilityAuthorizer, PathScopeAuthorizer};
    use cpe_security::{
        CombinePolicy, NullAudit, PlaneConfig, ProviderRegistry, SecurityChain, SecurityConfig,
        PASSTHROUGH,
    };
    use std::collections::BTreeMap;
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

    /// A chain that passes transport + authn (passthrough) but authorizes only paths under `root`
    /// (path-scope, all-must-pass), so it is *authorization* — not default-deny — that refuses an
    /// out-of-root call.
    fn path_scoped_chain(root: String) -> SecurityChain {
        let mut reg = ProviderRegistry::with_builtins();
        reg.register_authz("path_scope", move || Box::new(PathScopeAuthorizer::new([root.clone()])));
        let pass = |p: CombinePolicy, providers: Vec<String>| PlaneConfig { policy: p, providers };
        let config = SecurityConfig {
            transport: pass(CombinePolicy::FirstMatch, vec![PASSTHROUGH.into()]),
            authentication: pass(CombinePolicy::FirstMatch, vec![PASSTHROUGH.into()]),
            authorization: pass(CombinePolicy::AllMustPass, vec!["path_scope".into()]),
        };
        reg.build(&config, Box::new(NullAudit)).expect("path-scoped chain builds")
    }

    #[test]
    fn path_scope_authorizer_denies_out_of_root_over_the_wire() {
        // Distinct from default-deny: transport + authn pass, and the *authorizer* is what enforces —
        // proving the AuthZ plane (not just an unconfigured boundary) holds over the remote path.
        let root = scratch("scoped");
        std::fs::write(root.join("ok.txt"), b"x").unwrap();
        let addr = start_server(path_scoped_chain(root.to_string_lossy().into_owned()));
        let mut client = Client::connect(addr).unwrap();

        // A path under the granted root is authorized → dispatched (a real listing comes back).
        client
            .call("list_dir", serde_json::json!({ "path": root.to_string_lossy() }))
            .expect("in-scope path must be allowed and dispatched");

        // A path outside the root is refused at the authorization plane over the wire — the request
        // never reaches the filesystem, so a non-existent out-of-scope path denies (not NotFound).
        let err = client
            .call("list_dir", serde_json::json!({ "path": "/definitely/outside/the/root" }))
            .expect_err("out-of-scope path must be denied by the authorizer");
        assert!(
            matches!(err.code, ErrorCode::Unauthorized | ErrorCode::Unauthenticated),
            "expected an authorization denial, got {:?}: {}",
            err.code,
            err.message
        );
    }

    /// Plane config helper for the security-over-the-wire tests.
    fn plane(policy: CombinePolicy, providers: &[&str]) -> PlaneConfig {
        PlaneConfig { policy, providers: providers.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn capability_authorizer_denies_a_missing_capability_over_the_wire() {
        // Passthrough transport + authn; a capability authorizer that requires `fs.read` for `list_dir`
        // but grants it to nobody → the local principal is refused at the authorization plane remotely.
        let mut reg = ProviderRegistry::with_builtins();
        reg.register_authz("capability", || {
            let mut required = BTreeMap::new();
            required.insert("list_dir".to_string(), "fs.read".to_string());
            Box::new(CapabilityAuthorizer::new(required, BTreeMap::new())) // no grants
        });
        let config = SecurityConfig {
            transport: plane(CombinePolicy::FirstMatch, &[PASSTHROUGH]),
            authentication: plane(CombinePolicy::FirstMatch, &[PASSTHROUGH]),
            authorization: plane(CombinePolicy::AllMustPass, &["capability"]),
        };
        let addr = start_server(reg.build(&config, Box::new(NullAudit)).unwrap());
        let mut client = Client::connect(addr).unwrap();
        let err = client
            .call("list_dir", serde_json::json!({ "path": "/anything" }))
            .expect_err("a method needing an ungranted capability must be denied");
        assert!(
            matches!(err.code, ErrorCode::Unauthorized | ErrorCode::Unauthenticated),
            "expected an authorization denial, got {:?}: {}",
            err.code,
            err.message
        );
    }

    #[test]
    fn api_token_authn_refuses_an_unauthenticated_request_over_the_wire() {
        // AuthN requires a known API token (none is presented over the v1 wire) → the request is refused
        // at the authentication plane, distinct from a path/capability authorization denial.
        let mut reg = ProviderRegistry::with_builtins();
        reg.register_authn("api_token", || {
            let mut tokens = BTreeMap::new();
            tokens.insert("s3cr3t".to_string(), Principal { id: "alice".into(), display_name: None });
            Box::new(ApiTokenAuthenticator::new(tokens))
        });
        let config = SecurityConfig {
            transport: plane(CombinePolicy::FirstMatch, &[PASSTHROUGH]),
            authentication: plane(CombinePolicy::AnyPasses, &["api_token"]), // no passthrough authn
            authorization: plane(CombinePolicy::FirstMatch, &[PASSTHROUGH]),
        };
        let addr = start_server(reg.build(&config, Box::new(NullAudit)).unwrap());
        let mut client = Client::connect(addr).unwrap();
        let err = client
            .call("list_dir", serde_json::json!({ "path": "/anything" }))
            .expect_err("no credential must be refused at the authN plane");
        assert!(
            matches!(err.code, ErrorCode::Unauthenticated | ErrorCode::Unauthorized),
            "expected an authentication denial, got {:?}: {}",
            err.code,
            err.message
        );
    }

    /// Start a Server that also exposes a `count_stream` streaming method yielding `n` items.
    fn start_streaming_server(chain: SecurityChain) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let rt = Arc::new(
            ServerRuntime::new(
                Dispatcher::with_builtins(),
                chain,
                Arc::new(HeadlessCtx::new(scratch("streambase"))),
            )
            .with_stream_handler("count_stream", |_ctx, params| {
                let n = params.get("n").and_then(|v| v.as_u64()).unwrap_or(0);
                Ok((1..=n).map(|i| serde_json::json!({ "i": i })).collect())
            }),
        );
        std::thread::spawn(move || {
            let _ = rt.serve(listener);
        });
        addr
    }

    #[test]
    fn streaming_method_delivers_items_then_ends_over_the_wire() {
        let mut client = Client::connect(start_streaming_server(SecurityChain::local())).unwrap();
        let items = client
            .call_stream("count_stream", serde_json::json!({ "n": 3 }))
            .expect("stream should complete");
        assert_eq!(items.len(), 3, "three StreamItems before StreamEnd");
        assert_eq!(items[0]["i"], 1);
        assert_eq!(items[2]["i"], 3);
        // An empty stream is just an immediate StreamEnd (no items).
        assert!(client.call_stream("count_stream", serde_json::json!({ "n": 0 })).unwrap().is_empty());
    }

    #[test]
    fn list_dir_stream_streams_real_entries_over_the_wire() {
        // The builtin `list_dir_stream` producer streams one StreamItem per real DirEntry.
        let dir = scratch("liststream");
        std::fs::write(dir.join("a.txt"), b"a").unwrap();
        std::fs::write(dir.join("b.txt"), b"b").unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let rt = Arc::new(
            ServerRuntime::new(
                Dispatcher::with_builtins(),
                SecurityChain::local(),
                Arc::new(HeadlessCtx::new(scratch("liststreambase"))),
            )
            .with_builtin_streams(),
        );
        std::thread::spawn(move || {
            let _ = rt.serve(listener);
        });

        let mut client = Client::connect(addr).unwrap();
        let items = client
            .call_stream("list_dir_stream", serde_json::json!({ "path": dir.to_string_lossy() }))
            .expect("list_dir_stream should stream");
        let names: Vec<String> =
            items.iter().map(|e| e["name"].as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"a.txt".to_string()) && names.contains(&"b.txt".to_string()), "got {names:?}");
    }

    #[test]
    fn a_streaming_call_is_security_guarded_over_the_wire() {
        // The same stream method under a default-deny boundary yields no items — the denial arrives as
        // the stream's error, proving streaming enforces the security chain exactly like a unary call.
        let mut client = Client::connect(start_streaming_server(SecurityChain::default_deny())).unwrap();
        let err = client
            .call_stream("count_stream", serde_json::json!({ "n": 3 }))
            .expect_err("a denied stream must error, not stream items");
        assert!(matches!(err.code, ErrorCode::Unauthenticated | ErrorCode::Unauthorized));
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
    ///
    /// Each path is timed **best-of-`TRIALS`** and compared on its minimum batch time. A single
    /// batch sum can be spiked by one scheduler stall on a loaded runner (which made the naive
    /// `in_process < loopback` comparison flaky); the minimum over several trials is a stable
    /// lower-bound estimator that filters those one-off stalls, so only a genuine regression —
    /// the local path being consistently as slow as the networked one — trips the assert.
    #[test]
    fn in_process_dispatch_is_not_taxed_by_the_remote_path() {
        let dir = scratch("bench");
        std::fs::write(dir.join("a.txt"), b"hi").unwrap();
        let params = serde_json::json!({ "path": dir.to_string_lossy() });
        const N: usize = 200;
        const TRIALS: usize = 5;

        // In-process: direct dispatch, no socket — the local fast path.
        let ctx = HeadlessCtx::new(scratch("benchbase"));
        let dispatcher = Dispatcher::with_builtins();
        // Over loopback: the identical calls through the client/server.
        let addr = start_server(SecurityChain::local());
        let mut client = Client::connect(addr).unwrap();

        let mut best_in = Duration::MAX;
        let mut best_loop = Duration::MAX;
        for _ in 0..TRIALS {
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
            best_in = best_in.min(t0.elapsed());

            let t1 = Instant::now();
            for _ in 0..N {
                client.call("list_dir", params.clone()).unwrap();
            }
            best_loop = best_loop.min(t1.elapsed());
        }

        println!(
            "local-fast guard: best_in_process={best_in:?} best_loopback={best_loop:?} (N={N}, trials={TRIALS})"
        );
        assert!(
            best_in < best_loop,
            "the in-process path must stay faster than the remote path: best_in_process={best_in:?} best_loopback={best_loop:?}"
        );
    }
}
