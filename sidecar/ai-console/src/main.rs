//! The AI Console sidecar process (CPE-277/271). A thin stdio wrapper around the pure
//! protocol loop in the library: emit `Hello`, then read JSON-line envelopes and act on
//! each. On `Welcome` it also starts its **own UI** server (CPE-271) and announces the
//! loopback URL to the host via a `Status` event (`ui:<url>`), which the host embeds in
//! an iframe pane. Depends only on `sidecar-contract` + this crate's own modules.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ai_console::broker_client::{
    BrokerClient, BrokerDialogs, BrokerHistory, BrokerPresets, BrokerSecrets, HostDialogs,
    SharedWriter,
};
use ai_console::console::{route, ws_route, CatalogSources, ConsoleState, SessionAnnouncer};
use ai_console::history::HistoryBackend;
use ai_console::presets::PresetsBackend;
use ai_console::vault::SecretAccess;
use ai_console::{hello, http, on_message, Reaction};
use sidecar_contract::{Envelope, Event, Lifecycle, Message};

fn write_env(writer: &SharedWriter, env: &Envelope) {
    let mut out = writer.lock().unwrap();
    let _ = writeln!(out, "{}", env.to_json().expect("serialize"));
    let _ = out.flush();
}

/// Locate the bundled agent catalog: an explicit override, then `agents/` next to the
/// executable (how it ships), then the dev-tree copy.
fn agents_dir() -> PathBuf {
    if let Ok(p) = std::env::var("CPE_AICONSOLE_AGENTS") {
        let pb = PathBuf::from(p);
        if pb.is_dir() {
            return pb;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("agents");
            if p.is_dir() {
                return p;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents")
}

/// Locate the bundled reseller manifests (CPE-469), same resolution order as `agents_dir`.
fn resellers_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("resellers");
            if p.is_dir() {
                return p;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resellers")
}

/// Build the launcher's shared state: the agent catalog, the default folder, and the
/// secrets backend (the host keychain broker) so provider keys resolve at launch.
fn console_state(
    secrets: Arc<dyn SecretAccess + Send + Sync>,
    presets: Arc<dyn PresetsBackend>,
    dialogs: Arc<dyn HostDialogs>,
    history: Arc<dyn HistoryBackend>,
    announce: SessionAnnouncer,
) -> Arc<ConsoleState> {
    // Bundled first-party catalog, optionally layered with a verified, fetched/user-pointed source
    // (CPE-308). Only manifests signed by a configured trusted key are accepted; unset ⇒ bundled
    // only. Kept as `CatalogSources` so a catalog update can be hot-reloaded (CPE-375).
    let mut sources = CatalogSources { bundled: vec![agents_dir()], signed_dir: None, keys: vec![] };
    if let Ok(dir) = std::env::var("CPE_AICONSOLE_CATALOG") {
        sources.signed_dir = Some(PathBuf::from(dir));
        sources.keys = std::env::var("CPE_AICONSOLE_CATALOG_KEYS")
            .unwrap_or_default()
            .split([',', ' ', ';'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
    }
    let registry = sources.build();
    let cwd = std::env::var("CPE_AICONSOLE_CWD")
        .ok()
        .or_else(|| std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned()))
        .unwrap_or_default();
    // Launch-capable reseller descriptors (CPE-469): selecting one as the provider launches the agent
    // against that gateway via its protocol recipe.
    let resellers = ai_console::model_catalog::ResellerRegistry::load_from_dirs(&[resellers_dir()]).descriptors();
    let mut state = ConsoleState::with_backends(registry, cwd, secrets, presets, dialogs, history)
        .with_catalog_sources(sources)
        .with_resellers(resellers)
        .with_announcer(announce);

    // CPE-309 S4: sessions survive a console restart by living in a **host-owned** session daemon.
    // The host spawns that daemon with a hidden console (so Windows ConPTY produces output — the bug
    // that a DETACHED_PROCESS self-spawn hit) and outside this UI sidecar's lifetime (so it survives),
    // then passes its loopback address here. When present, route sessions to it and reattach any it
    // still holds; otherwise use the proven in-process engine.
    if let Ok(addr) = std::env::var("CPE_AICONSOLE_SESSION_DAEMON_ADDR") {
        if let Some(port) = addr.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) {
            let handle = ai_console::session_supervisor::SessionDaemonHandle::external(port);
            state = state.with_engine(Arc::new(ai_console::DaemonEngine::new(handle)));
        }
    }

    let state = Arc::new(state);
    // Reattach any sessions the daemon still holds from a previous console (CPE-309/461 across a full
    // restart) so their tabs come back with scrollback + live I/O. No-op for the in-process default.
    state.reattach_running_sessions();
    state
}

/// Run the process in session-daemon mode (CPE-309 slice 2): serve the PTY [`SessionDaemon`] over a
/// loopback socket forever. `--port <n>` pins the port (default 0 = OS-assigned); the chosen port is
/// printed as `PORT <n>` on stdout for the parent to read.
fn run_session_daemon() {
    let args: Vec<String> = std::env::args().collect();
    let port = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(0);
    let listener = match ai_console::session_server::bind(&format!("127.0.0.1:{port}")) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("session-daemon: bind failed: {e}");
            std::process::exit(1);
        }
    };
    if let Ok(addr) = listener.local_addr() {
        println!("PORT {}", addr.port());
        let _ = std::io::stdout().flush();
    }
    ai_console::session_server::serve(listener, Arc::new(ai_console::SessionDaemon::new()));
}

fn main() {
    // Session-daemon mode (CPE-309 slice 2): run only the PTY session daemon behind a loopback
    // socket, as its own long-lived process, so agent sessions survive a restart of the UI-sidecar
    // process (slices 3/4 wire the UI to it). Binds 127.0.0.1:<port|0>, prints `PORT <n>` so a
    // supervisor/parent learns the port, then serves until killed.
    if std::env::args().any(|a| a == "--session-daemon") {
        run_session_daemon();
        return;
    }

    let stdin = std::io::stdin();
    // A single shared writer so the main loop and the broker client never interleave
    // partial lines on stdout (CPE-344).
    let writer: SharedWriter = Arc::new(Mutex::new(Box::new(std::io::stdout())));
    // Outbound capability client (secrets.*). The loopback HTTP handlers call it; the
    // main loop below routes the host's responses back to it.
    let broker = Arc::new(BrokerClient::new(writer.clone()));

    write_env(&writer, &hello());

    // Kept alive for the process lifetime once the handshake completes.
    let mut _ui_server: Option<http::UiServer> = None;
    // A handle to the live console state so shutdown can reclaim every out-of-process resource
    // (agent PTYs) before the process exits — nothing left running on quit (CPE-442).
    let mut console: Option<Arc<ConsoleState>> = None;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let env = match Envelope::from_json(line.trim()) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // A response to one of our outbound capability requests: hand it to the waiter.
        if let Message::Response(resp) = &env.message {
            broker.deliver(env.id, resp.clone());
            continue;
        }

        // On Welcome: reach Ready, start the UI server, and announce its URL so the host
        // can mount it (CPE-271). Handled here (not in the pure `on_message`) because it
        // has side effects. The console gets a keychain-backed secrets store (CPE-344).
        if matches!(env.message, Message::Welcome(_)) {
            write_env(&writer, &Envelope::new(0, Message::Lifecycle(Lifecycle::Ready)));
            let secrets = Arc::new(BrokerSecrets::new(broker.clone()));
            let presets = Arc::new(BrokerPresets::new(broker.clone()));
            let dialogs = Arc::new(BrokerDialogs::new(broker.clone()));
            let history = Arc::new(BrokerHistory::new(broker.clone()));
            // Forward session lifecycle to the host as a `session:<json>` Status event so the
            // explorer can surface it in Agent Watch (CPE-396). Uses the same shared writer as
            // the main loop, so frames never interleave.
            let announce_writer = writer.clone();
            let announce: SessionAnnouncer = Arc::new(move |payload: String| {
                write_env(
                    &announce_writer,
                    &Envelope::new(0, Message::Event(Event::Status { state: format!("session:{payload}") })),
                );
            });
            let state = console_state(secrets, presets, dialogs, history, announce);
            if let Ok(server) = http::serve(Arc::clone(&state), route, ws_route) {
                write_env(
                    &writer,
                    &Envelope::new(0, Message::Event(Event::Status { state: format!("ui:{}", server.url()) })),
                );
                _ui_server = Some(server);
            }
            console = Some(state);
            continue;
        }

        match on_message(env) {
            Reaction::Send(reply) => write_env(&writer, &reply),
            Reaction::Exit(code) => {
                // Reclaim every out-of-process resource before we go — a `sidecar.shutdown` or host
                // `WillQuit` must not leave orphaned agent processes/PTYs behind (CPE-442).
                if let Some(c) = &console {
                    let _ = c.close_all();
                }
                std::process::exit(code);
            }
            Reaction::Nothing => {}
        }
    }
}
