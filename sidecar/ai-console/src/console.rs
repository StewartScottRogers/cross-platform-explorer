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
    seq: Mutex<u64>,
    /// Provider keys / credential profiles, brokered to the OS keychain (CPE-344).
    secrets: Arc<dyn crate::vault::SecretAccess + Send + Sync>,
    /// Launcher presets + remembered selection, persisted via host storage (CPE-352).
    presets: Arc<dyn crate::presets::PresetsBackend>,
}

/// The vault key a provider's shared API key is stored under (CPE-344/287). One key per
/// provider, reused by every agent that launches against it.
pub(crate) fn provider_secret_name(provider: &str) -> String {
    format!("provider:{provider}")
}

/// Which API key a launch uses: an explicit one (typed for this launch) wins; otherwise the
/// key stored for this provider; otherwise none (the agent's native login).
pub(crate) fn resolve_provider_key(
    secrets: &dyn crate::vault::SecretAccess,
    provider: &str,
    explicit: Option<String>,
) -> Option<String> {
    if explicit.is_some() {
        return explicit;
    }
    secrets.get(&provider_secret_name(provider)).ok().flatten()
}

impl ConsoleState {
    /// Standalone/dev state with in-memory backends (no host broker).
    pub fn new(registry: AgentRegistry, default_cwd: String) -> Self {
        Self::with_backends(
            registry,
            default_cwd,
            Arc::new(crate::broker_client::MemSecrets::default()),
            Arc::new(crate::presets::MemPresets::default()),
        )
    }

    /// State wired to real backends (the host keychain + storage brokers, in production).
    pub fn with_backends(
        registry: AgentRegistry,
        default_cwd: String,
        secrets: Arc<dyn crate::vault::SecretAccess + Send + Sync>,
        presets: Arc<dyn crate::presets::PresetsBackend>,
    ) -> Self {
        Self {
            registry,
            default_cwd,
            sessions: Mutex::new(HashMap::new()),
            seq: Mutex::new(0),
            secrets,
            presets,
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
        // Persisted presets + remembered selection (CPE-352), for the launcher to restore.
        let presets = serde_json::to_value(self.presets.load()).unwrap_or_else(|_| json!({}));
        json!({ "agents": agents_json, "cwd": self.default_cwd, "presets": presets })
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
        // Use the provider's stored key when the user didn't type one this launch — one key
        // shared across every agent using that provider (CPE-344/287).
        let api_key = resolve_provider_key(&*self.secrets, &provider, api_key);
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
        // Remember this selection for the agent so it's restored on next open (CPE-352).
        // Only the choice is stored (provider/model), never a key value.
        let mut store = self.presets.load();
        store.remember(
            agent_id,
            crate::presets::Preset {
                name: String::new(),
                provider: provider.clone(),
                model: v["model"].as_str().unwrap_or("").to_string(),
                small_model: v["smallModel"].as_str().unwrap_or("").to_string(),
                key_ref: None,
            },
        );
        let _ = self.presets.save(&store);

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

    // ---- launcher preset sets (CPE-353) ----

    /// Save (or update by name) a named set for an agent. The catalog already returns the
    /// stored sets, so the launcher re-reads them after this.
    fn handle_preset_save(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let agent = v["agent"].as_str().unwrap_or("").trim();
        let name = v["name"].as_str().unwrap_or("").trim();
        if agent.is_empty() || name.is_empty() {
            return bad("missing 'agent' or 'name'");
        }
        let preset = crate::presets::Preset {
            name: name.to_string(),
            provider: v["provider"].as_str().unwrap_or("").to_string(),
            model: v["model"].as_str().unwrap_or("").to_string(),
            small_model: v["smallModel"].as_str().unwrap_or("").to_string(),
            key_ref: v["keyRef"].as_str().map(str::to_string),
        };
        let mut store = self.presets.load();
        store.save_preset(agent, preset);
        match self.presets.save(&store) {
            Ok(()) => Response::json(json!({ "ok": true }).to_string()),
            Err(e) => bad(e),
        }
    }

    /// Delete a named set from an agent.
    fn handle_preset_delete(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let agent = v["agent"].as_str().unwrap_or("").trim();
        let name = v["name"].as_str().unwrap_or("").trim();
        if agent.is_empty() || name.is_empty() {
            return bad("missing 'agent' or 'name'");
        }
        let mut store = self.presets.load();
        store.delete_preset(agent, name);
        match self.presets.save(&store) {
            Ok(()) => Response::json(json!({ "ok": true }).to_string()),
            Err(e) => bad(e),
        }
    }

    // ---- provider credential management (CPE-345) ----

    /// Store a provider's API key in the OS keychain (via the broker). Format-checked
    /// first so an obviously-wrong paste is rejected before it's saved. One key per
    /// provider, shared across every agent that uses it.
    fn handle_key_set(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let provider = v["provider"].as_str().unwrap_or("").trim();
        let key = v["key"].as_str().unwrap_or("").trim();
        if provider.is_empty() {
            return bad("missing 'provider'");
        }
        if let Err(e) = crate::keycheck::check_key_format(provider, key) {
            return bad(e);
        }
        match self.secrets.set(&provider_secret_name(provider), key) {
            Ok(()) => Response::json(json!({ "ok": true }).to_string()),
            Err(e) => bad(e),
        }
    }

    /// List which providers have a stored key — NAMES ONLY, never a value. The keychain
    /// has no enumerate, so probe every provider the catalog knows about.
    fn handle_key_list(&self) -> Response {
        let mut providers: Vec<String> =
            self.registry.all().flat_map(|a| a.providers.iter().cloned()).collect();
        providers.sort();
        providers.dedup();
        let have: Vec<String> = providers
            .into_iter()
            .filter(|p| self.secrets.get(&provider_secret_name(p)).ok().flatten().is_some())
            .collect();
        Response::json(json!({ "providers": have }).to_string())
    }

    /// Remove a provider's stored key.
    fn handle_key_delete(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let provider = v["provider"].as_str().unwrap_or("").trim();
        if provider.is_empty() {
            return bad("missing 'provider'");
        }
        match self.secrets.delete(&provider_secret_name(provider)) {
            Ok(()) => Response::json(json!({ "ok": true }).to_string()),
            Err(e) => bad(e),
        }
    }

    /// Pre-check a key's format before saving. `live:false` marks this as an offline
    /// shape check, not a real provider call (that needs the Network capability — CPE-347).
    fn handle_key_verify(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let provider = v["provider"].as_str().unwrap_or("").trim();
        let key = v["key"].as_str().unwrap_or("");
        match crate::keycheck::check_key_format(provider, key) {
            Ok(()) => Response::json(
                json!({ "valid": true, "live": false, "detail": "Format looks valid." }).to_string(),
            ),
            Err(e) => Response::json(json!({ "valid": false, "live": false, "detail": e }).to_string()),
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
        ("POST", "/api/presets") => state.handle_preset_save(&req.body),
        ("POST", "/api/presets/delete") => state.handle_preset_delete(&req.body),
        ("GET", "/api/keys") => state.handle_key_list(),
        ("POST", "/api/keys") => state.handle_key_set(&req.body),
        ("POST", "/api/keys/delete") => state.handle_key_delete(&req.body),
        ("POST", "/api/keys/verify") => state.handle_key_verify(&req.body),
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

    #[test]
    fn provider_key_resolution_precedence() {
        use crate::broker_client::MemSecrets;
        use crate::vault::SecretAccess;
        let s = MemSecrets::default();
        // Nothing stored, nothing typed → no key (native login).
        assert_eq!(resolve_provider_key(&s, "openrouter", None), None);
        // A stored key is used when the user didn't type one…
        s.set(&provider_secret_name("openrouter"), "sk-stored").unwrap();
        assert_eq!(resolve_provider_key(&s, "openrouter", None).as_deref(), Some("sk-stored"));
        // …but an explicit key always wins…
        assert_eq!(
            resolve_provider_key(&s, "openrouter", Some("sk-typed".into())).as_deref(),
            Some("sk-typed"),
        );
        // …and a different provider isn't affected (namespaced by name).
        assert_eq!(resolve_provider_key(&s, "anthropic", None), None);
    }

    #[test]
    fn key_management_api_stores_lists_and_removes_without_leaking_values() {
        let st = state();
        let post = |path: &str, body: &str| {
            route(&st, &Request { method: "POST".into(), path: path.into(), body: body.into(), ..Default::default() })
        };

        // Store an OpenRouter key (valid format) → 200.
        assert_eq!(post("/api/keys", r#"{"provider":"openrouter","key":"sk-or-abcdef123456"}"#).status, 200);
        // A wrong-format key is rejected before it's ever stored → 400.
        assert_eq!(post("/api/keys", r#"{"provider":"openrouter","key":"nope"}"#).status, 400);

        // List reports the provider name — but NEVER the key value.
        let body = String::from_utf8_lossy(&get(&st, "/api/keys").body).to_string();
        assert!(body.contains("openrouter"));
        assert!(!body.contains("sk-or"), "the key list must not leak values");

        // Verify is an offline format pre-check (live:false).
        let v: Value = serde_json::from_slice(&post("/api/keys/verify", r#"{"provider":"openrouter","key":"sk-or-abcdef123456"}"#).body).unwrap();
        assert_eq!(v["valid"], true);
        assert_eq!(v["live"], false);

        // Delete removes it from the list.
        assert_eq!(post("/api/keys/delete", r#"{"provider":"openrouter"}"#).status, 200);
        assert!(!String::from_utf8_lossy(&get(&st, "/api/keys").body).contains("openrouter"));
    }

    #[test]
    fn preset_sets_api_saves_lists_and_deletes() {
        let st = state();
        let post = |path: &str, body: &str| {
            route(&st, &Request { method: "POST".into(), path: path.into(), body: body.into(), ..Default::default() })
        };
        // Save a named set for the claude agent.
        assert_eq!(
            post("/api/presets", r#"{"agent":"claude","name":"Work","provider":"openrouter","model":"sonnet"}"#).status,
            200,
        );
        // The catalog now carries it under presets.agents.claude.presets.
        let cat: Value = serde_json::from_slice(&get(&st, "/api/catalog").body).unwrap();
        let sets = &cat["presets"]["agents"]["claude"]["presets"];
        assert_eq!(sets[0]["name"], "Work");
        assert_eq!(sets[0]["provider"], "openrouter");
        assert_eq!(sets[0]["model"], "sonnet");
        // Missing agent/name is rejected.
        assert_eq!(post("/api/presets", r#"{"agent":"","name":"x"}"#).status, 400);
        // Delete removes it.
        assert_eq!(post("/api/presets/delete", r#"{"agent":"claude","name":"Work"}"#).status, 200);
        let cat2: Value = serde_json::from_slice(&get(&st, "/api/catalog").body).unwrap();
        let n = cat2["presets"]["agents"]["claude"]["presets"].as_array().map(|a| a.len()).unwrap_or(0);
        assert_eq!(n, 0);
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
