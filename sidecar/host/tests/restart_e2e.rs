//! Crash-injection / restart E2E (CPE-302).
//!
//! Kills a running sidecar and proves the supervisor primitives detect the death and
//! recover it with a fresh, healthy process — while the host (this test process)
//! stays up throughout. Complements `supervisor_e2e.rs` (happy path) and
//! `hello_sidecar_e2e.rs` (capability tour).

use std::collections::BTreeSet;

use sidecar_contract::{Capability, CONTRACT_VERSION};
use sidecar_host::supervisor::{handshake, spawn_process, Connection, RestartPolicy};

fn echo() -> String {
    env!("CARGO_BIN_EXE_echo_sidecar").to_string()
}

#[test]
fn a_crashed_sidecar_is_detected_and_restarted() {
    let consent: BTreeSet<Capability> = BTreeSet::new();

    // First life: spawn, handshake, healthy.
    let mut conn = spawn_process(&echo(), &[]).expect("spawn");
    handshake(&mut conn, CONTRACT_VERSION, &consent).expect("handshake");
    assert!(conn.is_alive());

    // Inject a crash by killing the process.
    conn.shutdown();
    assert!(!conn.is_alive(), "the host observes the sidecar has died");

    // The host is plainly still running (this code keeps executing), and the restart
    // policy permits a first restart...
    let policy = RestartPolicy::default();
    assert!(policy.delay_for(0).is_some());

    // ...so a respawn yields a fresh, healthy sidecar that handshakes cleanly.
    let mut restarted = spawn_process(&echo(), &[]).expect("respawn");
    let outcome = handshake(&mut restarted, CONTRACT_VERSION, &consent).expect("re-handshake");
    assert_eq!(outcome.sidecar_id, "echo");
    assert!(restarted.is_alive());
    restarted.shutdown();
}

#[test]
fn the_supervisor_gives_up_after_the_attempt_cap() {
    // A flapping sidecar must not be restarted forever — the policy eventually gives
    // up so the host can surface a failure instead of looping.
    let policy = RestartPolicy { max_attempts: 2, base_delay_ms: 10, max_delay_ms: 100 };
    assert!(policy.delay_for(0).is_some());
    assert!(policy.delay_for(1).is_some());
    assert!(policy.delay_for(2).is_none());
}
