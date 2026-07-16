//! CPE-309 S3/S4: the console's `DaemonEngine` drives sessions that live in the real
//! `--session-daemon` process, and can **reattach** a still-running session (the mechanism the
//! console uses on boot after a restart). Complements `session_reattach_across_restart.rs` (which
//! proves the raw client) by exercising the engine seam `console.rs` actually calls.

use std::collections::BTreeMap;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use ai_console::session_engine::{DaemonEngine, SessionEngine};
use ai_console::session_supervisor::SessionDaemonHandle;
use ai_console::PtyLaunch;

fn ready_then_tick() -> PtyLaunch {
    let (program, args) = if cfg!(target_os = "windows") {
        (
            "cmd".to_string(),
            vec![
                "/c".to_string(),
                "echo READY& ping -n 4 127.0.0.1 >NUL& echo TICK& ping -n 20 127.0.0.1 >NUL".to_string(),
            ],
        )
    } else {
        ("sh".to_string(), vec!["-c".to_string(), "echo READY; sleep 3; echo TICK; sleep 20".to_string()])
    };
    PtyLaunch { program, args, cwd: None, env: BTreeMap::new(), rows: 24, cols: 80 }
}

fn drain(rx: &Receiver<Vec<u8>>, marker: &str, timeout: Duration) -> String {
    let mut seen = String::new();
    let deadline = Instant::now() + timeout;
    while !seen.contains(marker) {
        let Some(rem) = deadline.checked_duration_since(Instant::now()) else { break };
        match rx.recv_timeout(rem.min(Duration::from_millis(300))) {
            Ok(b) => seen.push_str(&String::from_utf8_lossy(&b)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    seen
}

#[test]
fn daemon_engine_launches_in_the_daemon_process_and_reattaches_a_running_session() {
    let exe = std::path::PathBuf::from(env!("CARGO_BIN_EXE_ai-console"));
    // The engine owns the daemon child (the console's usual case); PTYs live in that process.
    let handle = SessionDaemonHandle::spawn(&exe).expect("spawn session daemon");
    let engine = DaemonEngine::new(handle);

    // Launch through the engine and read the session's first output.
    let io = engine.launch("s1", &ready_then_tick()).expect("engine launches in the daemon");
    let out = io.take_output().expect("output channel");
    assert!(drain(&out, "READY", Duration::from_secs(10)).contains("READY"), "no first output");

    // The engine reports the session as reattachable (what the console lists on boot) …
    assert!(engine.reattachable().contains(&"s1".to_string()), "session not listed for reattach");

    // … and a fresh attach (a new console instance would do this) recovers the replay + live output.
    let io2 = engine.attach("s1").expect("engine reattaches the running session");
    let out2 = io2.take_output().expect("reattached output channel");
    let seen = drain(&out2, "TICK", Duration::from_secs(20));
    assert!(seen.contains("READY"), "reattach lost the scrollback replay: {seen:?}");
    assert!(seen.contains("TICK"), "reattach got no live output: {seen:?}");

    io2.kill().expect("kill the recovered session");
    // Dropping the engine reaps the daemon child (owned handle).
    drop(engine);
}
