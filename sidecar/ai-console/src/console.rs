//! The AI Console launcher: HTTP API + served page that ties the agent registry, provider
//! routing, lifecycle, and PTY into the "agent × provider × model" launch surface
//! (CPE-289). The sidecar serves this on a loopback port; the host embeds it in a
//! sandboxed iframe (ADR 0001). All heavy lifting is delegated to the already-tested
//! modules — this layer is glue + JSON.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use serde_json::{json, Value};

use crate::agents::AgentRegistry;
use crate::http::{self, ws_op, Request, Response};
use crate::lifecycle::{self, RealRunner};
use crate::pty::PtySession;
use crate::routing::LaunchContext;
use crate::scope::{self, AgentLaunchRequest};

/// Server-side scrollback tail kept per session, so reopening the pane replays recent
/// history. Bounded so a very long session never grows host memory (CPE-334).
const RING_CAP: usize = 512 * 1024;

/// A live agent session. The PTY is kept alive (so the child isn't reaped); a reader
/// thread streams its output to the currently-attached WebSocket AND into a bounded ring
/// (for replay on reconnect). Input goes straight to the PTY writer.
struct Session {
    pty: Mutex<PtySession>,
    writer: Mutex<Box<dyn Write + Send>>,
    ring: Arc<Mutex<Vec<u8>>>,
    /// The attached terminal's output channel, if any (one pane at a time).
    live: Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
}

/// Shared console state, held behind an `Arc` and served across HTTP connections.
pub struct ConsoleState {
    registry: AgentRegistry,
    default_cwd: String,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    /// Last-used selection per repo (cwd) — powers "remember my choice" in the UI.
    last_used: Mutex<HashMap<String, Value>>,
    seq: Mutex<u64>,
}

impl ConsoleState {
    pub fn new(registry: AgentRegistry, default_cwd: String) -> Self {
        Self {
            registry,
            default_cwd,
            sessions: Mutex::new(HashMap::new()),
            last_used: Mutex::new(HashMap::new()),
            seq: Mutex::new(0),
        }
    }

    fn next_id(&self) -> String {
        let mut s = self.seq.lock().unwrap();
        *s += 1;
        format!("s{}", *s)
    }

    /// The agent/provider/model/install catalog for the launcher UI. Detects install state
    /// per agent via its manifest's detect command (missing/unfound ⇒ not installed).
    fn catalog(&self) -> Value {
        let agents: Vec<&crate::agents::AgentManifest> = self.registry.all().collect();
        // Probe install state for every agent IN PARALLEL. Each `detect` spawns a
        // subprocess (`<cli> --version`); probing 12 serially visibly stalls the launcher
        // (CPE-325). Scoped threads let us borrow the manifests without cloning.
        let detected: Vec<lifecycle::DetectResult> = std::thread::scope(|s| {
            let handles: Vec<_> = agents
                .iter()
                .map(|&a| s.spawn(move || lifecycle::detect(a, &RealRunner)))
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or(lifecycle::DetectResult { installed: false, version: None })
                })
                .collect()
        });
        let agents_json: Vec<Value> = agents
            .iter()
            .zip(detected)
            .map(|(a, det)| {
                json!({
                    "id": a.id,
                    "name": a.name,
                    "installed": det.installed,
                    "version": det.version,
                    "providers": a.providers,
                    "defaultModel": a.default_model,
                    "canInstall": a.install_for_current_os().is_some(),
                    "runnable": a.run_for_current_os().is_some(),
                })
            })
            .collect();
        let last = self.last_used.lock().unwrap().get(&self.default_cwd).cloned();
        json!({ "agents": agents_json, "cwd": self.default_cwd, "lastUsed": last })
    }

    fn handle_launch(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let agent_id = v["agent"].as_str().unwrap_or("");
        let provider = v["provider"].as_str().unwrap_or("").to_string();
        let model = str_opt(&v, "model");
        let small_model = str_opt(&v, "smallModel");
        let api_key = str_opt(&v, "apiKey");
        let base_url = str_opt(&v, "baseUrl");
        let cwd = str_opt(&v, "cwd").unwrap_or_else(|| self.default_cwd.clone());
        let extra_args: Vec<String> = v["extraArgs"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();

        let agent = match self.registry.get(agent_id) {
            Some(a) => a.clone(),
            None => return bad(format!("unknown agent '{agent_id}'")),
        };
        // LM Studio local provider (CPE-330): auto-detect a reachable endpoint and adopt
        // its actually-loaded model, so "agent × lmstudio-local" launches with no manual
        // URL entry. Any value the caller pinned still wins; if nothing is detected the
        // recipe's `base_url` default applies. Only pay the probe cost for this provider.
        let (base_url, model) = if provider == crate::lmstudio::PROVIDER_ID {
            crate::lmstudio::resolve_launch(base_url, model, crate::lmstudio::detect_default())
        } else {
            (base_url, model)
        };
        let ctx = LaunchContext { model, small_model, api_key, base_url };
        let req = AgentLaunchRequest {
            agent: &agent,
            provider: &provider,
            ctx,
            profile_env: BTreeMap::new(),
            cwd: cwd.clone(),
            extra_args,
            rows: 30,
            cols: 100,
        };
        let launch = match scope::build_launch(&req) {
            Ok(l) => l,
            Err(e) => return bad(e),
        };
        let dangerous = scope::dangerous_flags(&launch.args);

        let session = match PtySession::spawn(&launch) {
            Ok(s) => s,
            Err(e) => return bad(e),
        };
        let reader = match session.reader() {
            Ok(r) => r,
            Err(e) => return bad(e),
        };
        let writer = match session.writer() {
            Ok(w) => w,
            Err(e) => return bad(e),
        };

        let ring: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let live: Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>> = Arc::new(Mutex::new(None));
        {
            let ring = Arc::clone(&ring);
            let live = Arc::clone(&live);
            thread::spawn(move || {
                let mut reader = reader;
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let chunk = &buf[..n];
                            {
                                let mut r = ring.lock().unwrap();
                                r.extend_from_slice(chunk);
                                let over = r.len().saturating_sub(RING_CAP);
                                if over > 0 {
                                    r.drain(0..over);
                                }
                            }
                            // Push live to the attached terminal, if any (drop on send error).
                            if let Some(tx) = live.lock().unwrap().as_ref() {
                                let _ = tx.send(chunk.to_vec());
                            }
                        }
                    }
                }
            });
        }

        let id = self.next_id();
        self.sessions.lock().unwrap().insert(
            id.clone(),
            Arc::new(Session { pty: Mutex::new(session), writer: Mutex::new(writer), ring, live }),
        );
        self.last_used.lock().unwrap().insert(
            cwd,
            json!({ "agent": agent_id, "provider": provider, "model": v["model"] }),
        );

        Response::json(json!({ "session": id, "dangerousFlags": dangerous }).to_string())
    }

    fn handle_resize(&self, path: &str, body: &str) -> Response {
        let id = session_id_from(path, "/resize");
        let v: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({}));
        let rows = v["rows"].as_u64().unwrap_or(30) as u16;
        let cols = v["cols"].as_u64().unwrap_or(100) as u16;
        let sessions = self.sessions.lock().unwrap();
        let Some(sess) = sessions.get(&id) else { return bad("no such session") };
        let result = sess.pty.lock().unwrap().resize(rows, cols);
        match result {
            Ok(()) => Response::json("{\"ok\":true}"),
            Err(e) => bad(e),
        }
    }

    fn handle_install(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let agent_id = v["agent"].as_str().unwrap_or("");
        let agent = match self.registry.get(agent_id) {
            Some(a) => a.clone(),
            None => return bad(format!("unknown agent '{agent_id}'")),
        };
        match lifecycle::install(&agent, &RealRunner) {
            Ok(out) => Response::json(json!({ "ok": true, "output": out.stdout }).to_string()),
            Err(e) => bad(e),
        }
    }
}

/// Extract a non-empty string field from a JSON object.
fn str_opt(v: &Value, key: &str) -> Option<String> {
    v[key].as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

/// `/api/session/{id}/output` → `{id}`.
fn session_id_from(path: &str, suffix: &str) -> String {
    path.strip_prefix("/api/session/")
        .and_then(|p| p.strip_suffix(suffix))
        .unwrap_or("")
        .to_string()
}

fn bad(msg: impl Into<String>) -> Response {
    Response {
        status: 400,
        content_type: "application/json".into(),
        body: json!({ "error": msg.into() }).to_string().into_bytes(),
    }
}

/// Route one request to the console API or the launcher page.
pub fn route(state: &ConsoleState, req: &Request) -> Response {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => Response::html(launcher_html()),
        ("GET", "/api/catalog") => Response::json(state.catalog().to_string()),
        ("POST", "/api/launch") => state.handle_launch(&req.body),
        ("POST", "/api/install") => state.handle_install(&req.body),
        ("POST", p) if p.starts_with("/api/session/") && p.ends_with("/resize") => {
            state.handle_resize(p, &req.body)
        }
        // Terminal I/O is a WebSocket at /api/session/{id}/ws (handled by `ws_route`).
        _ => Response::not_found(),
    }
}

/// The WebSocket terminal pump (CPE-334): stream PTY output to the browser and pipe
/// browser input to the PTY, over one persistent connection. Owns `stream` for the
/// session. Replays the session's ring buffer on connect, then streams live.
pub fn ws_route(state: &ConsoleState, req: &Request, stream: TcpStream) {
    let id = session_id_from(&req.path, "/ws");
    let sess = match state.sessions.lock().unwrap().get(&id) {
        Some(s) => Arc::clone(s),
        None => return,
    };

    let write_stream = match stream.try_clone() {
        Ok(s) => Arc::new(Mutex::new(s)),
        Err(_) => return,
    };
    let mut read_stream = stream;

    // Register this connection as the session's live output sink, and grab a replay
    // snapshot of the ring under the same lock ordering.
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let replay = sess.ring.lock().unwrap().clone();
    *sess.live.lock().unwrap() = Some(tx);

    // Output thread: replay recent scrollback, then forward live chunks until the channel
    // closes (on disconnect) or the socket errors.
    let out_handle = {
        let ws = Arc::clone(&write_stream);
        thread::spawn(move || {
            if !replay.is_empty() {
                let mut w = ws.lock().unwrap();
                if http::ws_write_frame(&mut *w, ws_op::BINARY, &replay).is_err() {
                    return;
                }
            }
            while let Ok(chunk) = rx.recv() {
                let mut w = ws.lock().unwrap();
                if http::ws_write_frame(&mut *w, ws_op::BINARY, &chunk).is_err() {
                    break;
                }
            }
        })
    };

    // Input loop: browser keystrokes -> PTY. Answers pings; exits on close/EOF/error.
    while let Ok(Some(frame)) = http::ws_read_frame(&mut read_stream) {
        match frame.opcode {
            ws_op::TEXT | ws_op::BINARY => {
                if let Ok(mut w) = sess.writer.lock() {
                    let _ = w.write_all(&frame.payload);
                    let _ = w.flush();
                }
            }
            ws_op::PING => {
                let mut w = write_stream.lock().unwrap();
                let _ = http::ws_write_frame(&mut *w, ws_op::PONG, &frame.payload);
            }
            ws_op::CLOSE => break,
            _ => {}
        }
    }

    // Detach: drop our sender so the output thread ends, and clear the live sink.
    *sess.live.lock().unwrap() = None;
    let _ = out_handle.join();
}

/// The launcher page — self-contained HTML/CSS/JS with xterm.js inlined (no external
/// resources, so it works under the sandboxed iframe's opaque origin).
pub fn launcher_html() -> String {
    include_str!("launcher.html")
        .replace("/*__XTERM_CSS__*/", include_str!("vendor/xterm.css"))
        .replace("/*__XTERM_JS__*/", include_str!("vendor/xterm.js"))
        .replace("/*__FIT_JS__*/", include_str!("vendor/xterm-addon-fit.js"))
        .replace("/*__SEARCH_JS__*/", include_str!("vendor/xterm-addon-search.js"))
        .replace("/*__LINKS_JS__*/", include_str!("vendor/xterm-addon-web-links.js"))
        .replace("/*__UNICODE_JS__*/", include_str!("vendor/xterm-addon-unicode11.js"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn state() -> ConsoleState {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents");
        let registry = AgentRegistry::load_from_dirs(&[dir]);
        ConsoleState::new(registry, "/repo".into())
    }

    fn get(state: &ConsoleState, target: &str) -> Response {
        // Mirror the server: split the target into path + query as read_request does.
        let (path, query) = crate::http::parse_target(target);
        route(state, &Request { method: "GET".into(), path, query, ..Default::default() })
    }

    #[test]
    fn serves_the_launcher_page() {
        let r = get(&state(), "/");
        assert_eq!(r.status, 200);
        let body = String::from_utf8_lossy(&r.body);
        assert!(body.contains("AI Console"));
    }

    #[test]
    fn catalog_lists_agents_with_fields() {
        let r = get(&state(), "/api/catalog");
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        let agents = v["agents"].as_array().unwrap();
        assert!(!agents.is_empty(), "bundled agents should be listed");
        let first = &agents[0];
        assert!(first["id"].is_string());
        assert!(first["providers"].is_array());
        assert!(first["installed"].is_boolean());
        assert_eq!(v["cwd"], "/repo");
    }

    #[test]
    fn launch_rejects_unknown_agent() {
        let r = route(
            &state(),
            &Request {
                method: "POST".into(),
                path: "/api/launch".into(),
                body: r#"{"agent":"does-not-exist","provider":"native"}"#.into(),
                ..Default::default()
            },
        );
        assert_eq!(r.status, 400);
        assert!(String::from_utf8_lossy(&r.body).contains("unknown agent"));
    }

    #[test]
    fn resize_for_unknown_session_is_an_error() {
        let r = route(
            &state(),
            &Request {
                method: "POST".into(),
                path: "/api/session/nope/resize".into(),
                body: r#"{"rows":24,"cols":80}"#.into(),
                ..Default::default()
            },
        );
        assert_eq!(r.status, 400);
    }

    #[test]
    fn unknown_route_is_404() {
        assert_eq!(get(&state(), "/nope").status, 404);
    }

    // Real end-to-end WebSocket test: launch an echo "agent", connect a WS client, and
    // confirm the PTY output streams through (CPE-334). Spawns a subprocess + binds a real
    // port + is timing-sensitive, so it's a manual diagnostic (like ai_console_flow) rather
    // than a CI test that would flake under the parallel suite. Run:
    //   cargo test --lib ws_streams_pty_output_end_to_end -- --ignored --nocapture
    #[test]
    #[ignore = "spawns a process + binds a port; run manually"]
    fn ws_streams_pty_output_end_to_end() {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let agents = dir.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        let (cmd, args) = if cfg!(windows) {
            ("cmd", r#"["/c","echo","WS_STREAM_OK"]"#)
        } else {
            ("sh", r#"["-c","echo WS_STREAM_OK"]"#)
        };
        let manifest = format!(
            r#"{{"schema_version":1,"id":"echo","name":"Echo",
               "run":{{"windows":{{"command":"{cmd}","args":{args}}},
                       "macos":{{"command":"{cmd}","args":{args}}},
                       "linux":{{"command":"{cmd}","args":{args}}}}},
               "providers":["native"],
               "provider_recipes":{{"native":{{"env":{{}},"args":[]}}}}}}"#
        );
        std::fs::write(agents.join("echo.json"), manifest).unwrap();

        let state = Arc::new(ConsoleState::new(
            AgentRegistry::load_from_dirs(&[agents]),
            dir.path().to_string_lossy().into_owned(),
        ));
        let server = crate::http::serve(Arc::clone(&state), route, ws_route).unwrap();
        let port = server.port;

        // Launch over HTTP.
        let body = http_post(port, "/api/launch", r#"{"agent":"echo","provider":"native"}"#);
        let session = serde_json::from_str::<Value>(&body).unwrap()["session"]
            .as_str()
            .unwrap()
            .to_string();
        std::thread::sleep(Duration::from_millis(600)); // let echo run + land in the ring

        // WebSocket handshake.
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let req = format!(
            "GET /api/session/{session}/ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        s.write_all(req.as_bytes()).unwrap();
        let mut hdr = Vec::new();
        let mut b = [0u8; 1];
        while !hdr.ends_with(b"\r\n\r\n") {
            if s.read(&mut b).unwrap() == 0 {
                break;
            }
            hdr.push(b[0]);
        }
        let resp = String::from_utf8_lossy(&hdr);
        assert!(resp.contains("101"), "handshake failed: {resp}");
        assert!(resp.contains(&crate::http::ws_accept_key(key)), "bad accept: {resp}");

        // Read frames until we see the echoed marker.
        let mut got = Vec::new();
        for _ in 0..20 {
            match crate::http::ws_read_frame(&mut s) {
                Ok(Some(f)) if f.opcode == crate::http::ws_op::BINARY => {
                    got.extend_from_slice(&f.payload);
                    if String::from_utf8_lossy(&got).contains("WS_STREAM_OK") {
                        break;
                    }
                }
                Ok(Some(_)) => {}
                _ => break,
            }
        }
        assert!(
            String::from_utf8_lossy(&got).contains("WS_STREAM_OK"),
            "WS did not stream PTY output; got {:?}",
            String::from_utf8_lossy(&got)
        );
    }

    // Diagnostic: does Claude Code switch to the terminal's ALTERNATE screen buffer? If it
    // does (ESC[?1049h / ?47h / ?1047h), there is no xterm scrollback to attach a scrollbar
    // to — the agent owns its scrolling. Run:
    //   cargo test --lib probe_claude_altscreen -- --ignored --nocapture
    //
    // CONFIRMED (CPE-337, 2026-07-13): ALT_SCREEN = true; Claude also enables full mouse
    // tracking (?1000h/?1002h/?1003h/?1006h). Conclusion: the custom scrollbar in
    // launcher.html correctly self-hides under such agents because xterm keeps baseY == 0 in
    // the alt buffer, and wheel events pass through to the agent. No frontend change needed.
    #[test]
    #[ignore = "probe: needs claude installed; spawns the real TUI"]
    fn probe_claude_altscreen() {
        use crate::routing::LaunchContext;
        use crate::scope::{build_launch, AgentLaunchRequest};
        use std::io::Read;
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents");
        let agent = AgentRegistry::load_from_dirs(&[dir]).get("claude").unwrap().clone();
        let req = AgentLaunchRequest {
            agent: &agent,
            provider: "native",
            ctx: LaunchContext::default(),
            profile_env: BTreeMap::new(),
            cwd: ".".into(),
            extra_args: vec![],
            rows: 40,
            cols: 120,
        };
        let launch = build_launch(&req).unwrap();
        let mut session = crate::pty::PtySession::spawn(&launch).unwrap();
        let mut reader = session.reader().unwrap();
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut b = [0u8; 8192];
            loop {
                match reader.read(&mut b) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(b[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let start = Instant::now();
        let mut out = Vec::new();
        while start.elapsed() < Duration::from_secs(3) {
            if let Ok(c) = rx.recv_timeout(Duration::from_millis(200)) {
                out.extend_from_slice(&c);
            }
        }
        let _ = session.kill();
        let alt = out.windows(8).any(|w| w == b"\x1b[?1049h")
            || out.windows(8).any(|w| w == b"\x1b[?1047h")
            || out.windows(6).any(|w| w == b"\x1b[?47h");
        let head: String = out.iter().take(160).map(|b| format!("{b:02x}")).collect();
        eprintln!("=== ALT_SCREEN = {alt} ===");
        eprintln!("=== bytes read = {} ===", out.len());
        eprintln!("=== head hex = {head} ===");
    }

    fn http_post(port: u16, path: &str, body: &str) -> String {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let req = format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        s.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        resp.split("\r\n\r\n").nth(1).unwrap_or("").to_string()
    }
}
