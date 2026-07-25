//! CPE-309: the `SessionDaemonHandle` supervisor spawns the REAL `--session-daemon` process, learns
//! its port, and hands out a working `SessionClient` — the console-side building block for
//! cross-restart reattach. Proves the spawn/connect/reap wiring against a live child process.

use std::path::PathBuf;

use ai_console::session_supervisor::SessionDaemonHandle;

#[test]
fn supervisor_spawns_the_daemon_and_a_client_can_reach_it() {
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_ai-console"));
    let mut handle = SessionDaemonHandle::spawn(&exe).expect("spawn session daemon");
    assert!(handle.port() > 0, "daemon should announce a port");
    assert!(handle.is_running(), "daemon child should be alive");

    // A client connects over the real socket; a fresh daemon holds no sessions.
    let client = handle.client().expect("connect a client to the daemon");
    assert!(client.list().expect("list sessions").is_empty());

    // Dropping the handle kills + reaps the child.
    drop(client);
    drop(handle);
}
