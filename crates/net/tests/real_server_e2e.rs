//! Real, non-loopback client<->server E2E (CPE-1659 slice 2, epic CPE-1498, retiring
//! MANUAL-TEST-BURNDOWN row #7 — "real remote network run").
//!
//! `crates/net`'s existing coverage (`src/lib.rs`'s unit tests, `examples/client.rs`) only ever drives
//! `cpe-server-ref` over `127.0.0.1` in the SAME process's network namespace. This test drives the
//! REAL `cpe-server-ref` binary running inside a Docker container on the CI job's bridge network, from
//! THIS test process (running directly on the runner, outside any container) at the container's own
//! address — a genuinely different network namespace, never `127.0.0.1` — which is the actual thing
//! burndown row #7 has asked for since 2026-07-25.
//!
//! `#[ignore]`d so `cargo test` in `crates/net` (part of the existing 3-OS `Server crates` CI matrix)
//! is unchanged; this only runs in the `Network E2E (ubuntu-latest, real servers)` CI job
//! (`.github/workflows/ci.yml`), which builds `cpe-server-ref` into a container, starts it, and sets
//! the `CPE_E2E_NET_*` environment variables read below.

use cpe_contract::ErrorCode;
use cpe_net::Client;

fn env_var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!(
            "{name} is not set — this test only runs inside the CPE-1659 Docker rig (job \
             `Network E2E (ubuntu-latest, real servers)` in .github/workflows/ci.yml); it is not \
             meant to run standalone."
        )
    })
}

/// The small fixture the CI workflow seeds into `CPE_E2E_NET_FIXTURE_DIR` (on the runner's own disk)
/// and bind-mounts into the `cpe-server-ref` container at `CPE_E2E_NET_CONTAINER_DIR`.
const SEEDED_FILES: &[&str] = &["alpha.txt", "beta.txt"];

#[test]
#[ignore = "needs the CPE-1659 docker rig — see .github/workflows/ci.yml"]
fn cpe_net_client_lists_a_real_directory_over_a_non_loopback_container_ip() {
    let addr = env_var("CPE_E2E_NET_ADDR"); // "<container-ip>:<port>", e.g. "172.28.0.14:9876"
    let remote_dir = std::env::var("CPE_E2E_NET_CONTAINER_DIR").unwrap_or_else(|_| "/data".to_string());

    // The whole point of slice 2: this must be a real, routed, non-loopback address in a different
    // network namespace from this test process — not the loopback shortcut every other cpe-net test uses.
    assert!(
        !addr.starts_with("127.0.0.1") && !addr.starts_with("localhost") && !addr.starts_with("[::1]"),
        "burndown row #7 specifically requires a non-loopback container address; got {addr}"
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut client = loop {
        match Client::connect(&addr) {
            Ok(c) => break c,
            Err(e) if std::time::Instant::now() < deadline => {
                let _ = e;
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
            Err(e) => panic!("could not connect to the real cpe-server-ref container at {addr}: {e}"),
        }
    };

    // A real listing crosses the socket from a genuinely different network namespace.
    let value = client
        .call("list_dir", serde_json::json!({ "path": remote_dir }))
        .expect("list_dir over the real container network must succeed");
    let entries = value.as_array().cloned().unwrap_or_default();
    let names: Vec<String> =
        entries.iter().filter_map(|e| e["name"].as_str().map(str::to_string)).collect();
    for expected in SEEDED_FILES {
        assert!(names.iter().any(|n| n == expected), "expected {expected:?} in {names:?}");
    }

    // Error path: a path that doesn't exist inside the container's own filesystem must come back as a
    // structured NotFound over the real socket — not a panic, not a hang, not a silent empty success.
    let err = client
        .call("list_dir", serde_json::json!({ "path": format!("{remote_dir}/does-not-exist") }))
        .expect_err("a missing remote path must be a clean error over the real network, not a silent success");
    assert_eq!(err.code, ErrorCode::NotFound, "expected NotFound, got {:?}: {}", err.code, err.message);
}
