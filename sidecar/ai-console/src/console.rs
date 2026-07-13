//! The AI Console launcher: HTTP API + served page that ties the agent registry, provider
//! routing, lifecycle, and PTY into the "agent × provider × model" launch surface
//! (CPE-289). The sidecar serves this on a loopback port; the host embeds it in a
//! sandboxed iframe (ADR 0001). All heavy lifting is delegated to the already-tested
//! modules — this layer is glue + JSON.

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::agents::AgentRegistry;
use crate::http::{Request, Response};
use crate::lifecycle::{self, RealRunner};
use crate::pty::PtySession;
use crate::routing::LaunchContext;
use crate::scope::{self, AgentLaunchRequest};

/// A live agent session: the PTY (kept alive so the child isn't reaped), an append-only
/// output buffer fed by a reader thread, and a handle to the PTY's input.
struct Session {
    _pty: Mutex<PtySession>,
    buffer: Arc<Mutex<Vec<u8>>>,
    writer: Mutex<Box<dyn Write + Send>>,
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
        let runner = RealRunner;
        let agents: Vec<Value> = self
            .registry
            .all()
            .map(|a| {
                let det = lifecycle::detect(a, &runner);
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
        json!({ "agents": agents, "cwd": self.default_cwd, "lastUsed": last })
    }

    fn handle_launch(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let agent_id = v["agent"].as_str().unwrap_or("");
        let provider = v["provider"].as_str().unwrap_or("").to_string();
        let model = str_opt(&v, "model");
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
        let ctx = LaunchContext { model, small_model: None, api_key, base_url };
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

        let buffer = Arc::new(Mutex::new(Vec::new()));
        {
            let buffer = Arc::clone(&buffer);
            std::thread::spawn(move || {
                let mut reader = reader;
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buffer.lock().unwrap().extend_from_slice(&buf[..n]),
                    }
                }
            });
        }

        let id = self.next_id();
        self.sessions.lock().unwrap().insert(
            id.clone(),
            Arc::new(Session { _pty: Mutex::new(session), buffer, writer: Mutex::new(writer) }),
        );
        self.last_used.lock().unwrap().insert(
            cwd,
            json!({ "agent": agent_id, "provider": provider, "model": v["model"] }),
        );

        Response::json(json!({ "session": id, "dangerousFlags": dangerous }).to_string())
    }

    fn handle_output(&self, path: &str, since: Option<&str>) -> Response {
        let id = session_id_from(path, "/output");
        let since: usize = since.and_then(|s| s.parse().ok()).unwrap_or(0);
        let sessions = self.sessions.lock().unwrap();
        let Some(sess) = sessions.get(&id) else { return bad("no such session") };
        let buf = sess.buffer.lock().unwrap();
        let slice = if since <= buf.len() { &buf[since..] } else { &[][..] };
        let text = String::from_utf8_lossy(slice).into_owned();
        Response::json(json!({ "offset": buf.len(), "data": text }).to_string())
    }

    fn handle_input(&self, path: &str, body: &str) -> Response {
        let id = session_id_from(path, "/input");
        let sessions = self.sessions.lock().unwrap();
        let Some(sess) = sessions.get(&id) else { return bad("no such session") };
        let mut w = sess.writer.lock().unwrap();
        match w.write_all(body.as_bytes()).and_then(|_| w.flush()) {
            Ok(()) => Response::json("{\"ok\":true}"),
            Err(e) => bad(format!("write failed: {e}")),
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
        ("GET", p) if p.starts_with("/api/session/") && p.ends_with("/output") => {
            state.handle_output(p, req.query("since"))
        }
        ("POST", p) if p.starts_with("/api/session/") && p.ends_with("/input") => {
            state.handle_input(p, &req.body)
        }
        _ => Response::not_found(),
    }
}

/// The launcher page — self-contained HTML/CSS/JS (no external resources, so it works under
/// the sandboxed iframe's opaque origin).
pub fn launcher_html() -> String {
    include_str!("launcher.html").to_string()
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
    fn output_for_unknown_session_is_an_error() {
        let r = get(&state(), "/api/session/nope/output?since=0");
        assert_eq!(r.status, 400);
    }

    #[test]
    fn unknown_route_is_404() {
        assert_eq!(get(&state(), "/nope").status, 404);
    }
}
