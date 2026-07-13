//! End-to-end supervisor tests (CPE-265) against a REAL child process — the bundled
//! `echo_sidecar` binary. This validates the whole stack over an actual OS process
//! boundary: spawn → JSON-line stdio transport → handshake, plus the conformance kit
//! (CPE-301) driving the real process, and liveness/shutdown.

use std::collections::BTreeSet;

use sidecar_contract::{Capability, CONTRACT_VERSION};
use sidecar_host::conformance::run_conformance;
use sidecar_host::supervisor::{handshake, spawn_process, Connection};

/// Path to the compiled echo_sidecar binary (Cargo sets this for bin targets).
fn echo_bin() -> String {
    env!("CARGO_BIN_EXE_echo_sidecar").to_string()
}

#[test]
fn conformance_kit_passes_against_a_real_process() {
    let mut conn = spawn_process(&echo_bin(), &[]).expect("spawn echo_sidecar");
    let report = run_conformance(&mut conn, CONTRACT_VERSION);
    assert!(
        report.passed(),
        "conformance failures: {:?}",
        report.failures().collect::<Vec<_>>()
    );
    conn.shutdown();
}

#[test]
fn supervisor_handshake_completes_against_a_real_process() {
    let mut conn = spawn_process(&echo_bin(), &[]).expect("spawn echo_sidecar");
    let consented: BTreeSet<Capability> = [Capability::Context].into_iter().collect();
    let outcome = handshake(&mut conn, CONTRACT_VERSION, &consented).expect("handshake");
    assert_eq!(outcome.sidecar_id, "echo");
    assert_eq!(outcome.negotiated.major, CONTRACT_VERSION.major);
    // echo requests only Context; consent grants it.
    assert!(outcome.granted.contains(&Capability::Context));
    conn.shutdown();
}

#[test]
fn liveness_and_shutdown_track_the_real_process() {
    let mut conn = spawn_process(&echo_bin(), &[]).expect("spawn echo_sidecar");
    assert!(conn.is_alive(), "freshly spawned process should be alive");
    conn.shutdown();
    // After shutdown the process is gone.
    assert!(!conn.is_alive(), "process should be dead after shutdown");
}
