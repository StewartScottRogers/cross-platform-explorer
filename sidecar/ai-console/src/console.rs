//! The AI Console launcher: HTTP API + served page that ties the agent registry, provider
//! routing, lifecycle, and PTY into the "agent × provider × model" launch surface
//! (CPE-289). The sidecar serves this on a loopback port; the host embeds it in a
//! sandboxed iframe (ADR 0001). All heavy lifting is delegated to the already-tested
//! modules — this layer is glue + JSON.

use std::collections::{BTreeMap, HashMap};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::thread;

use serde_json::{json, Value};

use crate::agents::AgentRegistry;
use crate::http::{self, ws_op, Request, Response};
use crate::lifecycle::{self, RealRunner};
use crate::routing::LaunchContext;
use crate::scope::{self, AgentLaunchRequest};

/// Server-side scrollback tail kept per session, so reopening the pane replays recent
/// history. Bounded so a very long session never grows host memory (CPE-334).
const RING_CAP: usize = 512 * 1024;

/// A live agent session. Its PTY lives wherever the [`SessionEngine`] puts it — in-process
/// (`LocalEngine`) or in the session daemon (`DaemonEngine`, so it survives a console restart,
/// CPE-309). A reader thread streams the engine's output channel to the attached WebSocket AND into a
/// bounded ring (for replay on reconnect). Input/resize/kill go through the engine's `SessionIo`.
struct Session {
    io: Arc<dyn crate::session_engine::SessionIo>,
    ring: Arc<Mutex<Vec<u8>>>,
    /// The attached terminal's output channel, if any (one pane at a time).
    live: Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>>,
    /// The tab label (agent · provider · model), so the launcher can **reattach** a tab to this
    /// still-running session when the AI Console is closed and reopened (CPE-461).
    name: String,
    /// Provider-reported token/cost usage for this session, accumulated from the output stream by the
    /// reader thread (CPE-311). Surfaced via `/api/sessions`; never sent anywhere.
    usage: Arc<Mutex<crate::usage::Usage>>,
    /// Identity (agent/provider) so usage can be aggregated per agent/provider (CPE-311).
    agent: String,
    provider: String,
}

/// The reproducible identity of a launched session — captured at launch, recorded to history when
/// the session ends so its transcript survives a restart and it can be relaunched (CPE-370).
#[derive(Clone)]
struct SessionMeta {
    agent: String,
    provider: String,
    model: String,
    cwd: String,
    /// Epoch-millis string (opaque to history; the launcher formats it).
    started_at: String,
}

/// Shared console state, held behind an `Arc` and served across HTTP connections.
pub struct ConsoleState {
    /// Hot-swappable so a catalog update can be reloaded without a restart (CPE-375).
    registry: RwLock<AgentRegistry>,
    default_cwd: String,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    /// Where session PTYs live (CPE-309): `LocalEngine` in-process by default; the real sidecar wires
    /// a `DaemonEngine` so sessions survive a console restart.
    engine: Arc<dyn crate::session_engine::SessionEngine>,
    seq: Mutex<u64>,
    /// Provider keys / credential profiles, brokered to the OS keychain (CPE-344).
    secrets: Arc<dyn crate::vault::SecretAccess + Send + Sync>,
    /// Launcher presets + remembered selection, persisted via host storage (CPE-352).
    presets: Arc<dyn crate::presets::PresetsBackend>,
    /// Host-mediated dialogs (native folder picker) for the sandboxed launcher (CPE-354).
    dialogs: Arc<dyn crate::broker_client::HostDialogs>,
    /// Persisted session history so sessions + transcripts survive a restart (CPE-370/292).
    history: Arc<dyn crate::history::HistoryBackend>,
    /// How to rebuild the registry on reload (CPE-375): the bundled dirs + optional verified source.
    sources: CatalogSources,
    /// Announce session lifecycle to the host so the explorer can surface it (Agent Watch, CPE-396).
    /// Each call gets a JSON payload; the wire side (main.rs) turns it into a `session:<json>`
    /// Status event. Defaults to a no-op so dev/standalone + tests need no host.
    announce: SessionAnnouncer,
    /// The last **verified** model-catalog snapshot the picker serves from — `(version, models)`
    /// (CPE-451). Downloaded host-mediated, ed25519-verified, and anti-rollback-gated before it lands
    /// here; `None` until a good one is adopted, in which case the picker uses the live per-reseller
    /// fetch. Fast + offline once populated.
    snapshot_models: Mutex<Option<(u64, Vec<crate::model_catalog::Model>)>>,
    /// Trusted first-party public keys (hex) a downloaded snapshot must be signed by (CPE-451).
    /// Production is the single hardcoded [`MODEL_CATALOG_TRUSTED_KEY`]; tests inject their own via
    /// [`ConsoleState::with_snapshot_keys`].
    snapshot_keys: Vec<String>,
    /// Reseller launch descriptors (CPE-469): if the selected provider matches one of these ids and
    /// the agent speaks its protocol, `handle_launch` routes through `compose_reseller_launch`.
    resellers: Vec<crate::routing::ResellerDescriptor>,
}

/// Hook the console calls to announce session start/end to the host (CPE-396). No-op by default.
pub type SessionAnnouncer = Arc<dyn Fn(String) + Send + Sync>;

/// The inputs used to (re)build the agent registry, so a catalog update can be hot-reloaded
/// without a restart (CPE-375). `bundled` dirs are first-party (loaded as-is); `signed_dir` is an
/// untrusted source whose manifests must each be signed by one of `keys` (CPE-371).
#[derive(Clone, Default)]
pub struct CatalogSources {
    pub bundled: Vec<PathBuf>,
    pub signed_dir: Option<PathBuf>,
    pub keys: Vec<String>,
}

impl CatalogSources {
    /// Build a fresh registry: the bundled dirs, then the verified signed source layered over them.
    pub fn build(&self) -> AgentRegistry {
        let mut reg = AgentRegistry::load_from_dirs(&self.bundled);
        if let Some(dir) = &self.signed_dir {
            reg.load_signed_source(dir, &self.keys);
        }
        reg
    }

    fn is_empty(&self) -> bool {
        self.bundled.is_empty() && self.signed_dir.is_none()
    }
}

/// The implicit credential label — a provider's single/primary key (CPE-348).
pub(crate) const DEFAULT_CREDENTIAL: &str = "default";

/// The trusted first-party ed25519 public key (hex) the downloaded model-catalog snapshot must be
/// signed by (CPE-451). Same public value as the host's `CATALOG_TRUSTED_KEYS`; a public key is safe
/// to embed. The matching private seed is the `CPE_CATALOG_SIGNING_KEY` release secret, never here.
pub(crate) const MODEL_CATALOG_TRUSTED_KEY: &str =
    "5b18ad467b37b7c06556000f15359a845bd85790ece91de110a337890d017130";

/// The vault key a provider's API key is stored under (CPE-344/287/348). The `default` label
/// keeps the original `provider:<id>` name (back-compat); other labels get `provider:<id>#<label>`
/// so a provider can hold several credentials.
pub(crate) fn provider_secret_name(provider: &str, label: &str) -> String {
    if label.is_empty() || label == DEFAULT_CREDENTIAL {
        format!("provider:{provider}")
    } else {
        format!("provider:{provider}#{label}")
    }
}

/// Keychain name for a model **reseller's** key (CPE-452), namespaced apart from provider keys so a
/// reseller credential never collides with a provider one. Resolved for the allow-listed model-list
/// egress (CPE-447) + inference; the value never leaves the keychain broker.
pub(crate) fn reseller_secret_name(reseller: &str, label: &str) -> String {
    if label.is_empty() || label == DEFAULT_CREDENTIAL {
        format!("reseller:{reseller}")
    } else {
        format!("reseller:{reseller}#{label}")
    }
}

/// Which API key a launch uses: an explicit one (typed for this launch) wins; otherwise the
/// stored key for this provider under `label`; otherwise none (the agent's native login).
pub(crate) fn resolve_provider_key(
    secrets: &dyn crate::vault::SecretAccess,
    provider: &str,
    explicit: Option<String>,
    label: &str,
) -> Option<String> {
    if explicit.is_some() {
        return explicit;
    }
    secrets.get(&provider_secret_name(provider, label)).ok().flatten()
}

/// Build a session lifecycle announcement for the explorer's Agent Watch (CPE-396). `event` is
/// "started" or "ended"; the payload carries just enough to list + locate the session (its
/// Project folder is `cwd`). No secrets are ever included.
/// Build the `fs-read:<json>` announcement for a file the agent reported reading (CPE-405).
/// The raw captured path (relative or absolute) is resolved against the session's Project folder
/// (`cwd`) so it matches the absolute paths the host's FS watcher emits (CPE-398); the host forwards
/// it onto the `ai-console://fs-activity` channel with `kind:"read"`.
fn read_announcement(cwd: &str, raw: &str) -> String {
    let abs = std::path::Path::new(cwd).join(raw).to_string_lossy().into_owned();
    format!("fs-read:{}", json!({ "path": abs }))
}

/// JSON view of a session's usage (CPE-311). Omitted fields stay 0; the launcher hides an all-zero
/// readout so a session whose agent prints no usage shows nothing.
fn usage_json(u: &crate::usage::Usage) -> Value {
    json!({
        "inputTokens": u.input_tokens,
        "outputTokens": u.output_tokens,
        "costUsd": u.cost_usd,
    })
}

/// Sum one session's usage into an aggregate bucket (per agent / per provider). Across sessions the
/// figures **add** (each session's cost is independent), unlike the within-session max.
fn accumulate_usage(acc: &mut crate::usage::Usage, u: &crate::usage::Usage) {
    acc.input_tokens += u.input_tokens;
    acc.output_tokens += u.output_tokens;
    acc.cost_usd += u.cost_usd;
}

fn session_payload(event: &str, id: &str, agent_name: &str, meta: &SessionMeta) -> String {
    json!({
        "event": event,
        "sessionId": id,
        "agentId": meta.agent,
        "agentName": agent_name,
        "provider": meta.provider,
        "model": meta.model,
        "cwd": meta.cwd,
    })
    .to_string()
}

impl ConsoleState {
    /// Standalone/dev state with in-memory backends (no host broker).
    pub fn new(registry: AgentRegistry, default_cwd: String) -> Self {
        Self::with_backends(
            registry,
            default_cwd,
            Arc::new(crate::broker_client::MemSecrets::default()),
            Arc::new(crate::presets::MemPresets::default()),
            Arc::new(crate::broker_client::NoopDialogs),
            Arc::new(crate::history::MemHistory::default()),
        )
    }

    /// State wired to real backends (the host keychain + storage + dialog brokers).
    pub fn with_backends(
        registry: AgentRegistry,
        default_cwd: String,
        secrets: Arc<dyn crate::vault::SecretAccess + Send + Sync>,
        presets: Arc<dyn crate::presets::PresetsBackend>,
        dialogs: Arc<dyn crate::broker_client::HostDialogs>,
        history: Arc<dyn crate::history::HistoryBackend>,
    ) -> Self {
        Self {
            registry: RwLock::new(registry),
            default_cwd,
            sessions: Mutex::new(HashMap::new()),
            engine: Arc::new(crate::session_engine::LocalEngine),
            seq: Mutex::new(0),
            secrets,
            presets,
            dialogs,
            history,
            sources: CatalogSources::default(),
            announce: Arc::new(|_| {}),
            snapshot_models: Mutex::new(None),
            snapshot_keys: vec![MODEL_CATALOG_TRUSTED_KEY.to_string()],
            resellers: Vec::new(),
        }
    }

    /// Provide the reseller launch descriptors (CPE-469) — the launch-capable resellers from the
    /// bundled `resellers/*.json`. Chained after construction.
    pub fn with_resellers(mut self, resellers: Vec<crate::routing::ResellerDescriptor>) -> Self {
        self.resellers = resellers;
        self
    }

    /// Swap the session engine (CPE-309). The real sidecar passes a `DaemonEngine` so session PTYs
    /// live in the long-lived daemon process and survive a console restart; dev/tests keep the
    /// in-process `LocalEngine`. Chained after construction.
    pub fn with_engine(mut self, engine: Arc<dyn crate::session_engine::SessionEngine>) -> Self {
        self.engine = engine;
        self
    }

    /// On console boot, re-open a tab for every session still alive in the engine (CPE-309/461 across
    /// a full process restart): the daemon keeps the PTYs, so we `attach` each and wire the same
    /// reader pipeline as a fresh launch. Local engines hold nothing, so this is a no-op there.
    /// Returns the ids reattached. The tab name defaults to the id (names were console-side and don't
    /// survive the restart; the scrollback + live I/O do, which is the point).
    pub fn reattach_running_sessions(self: &Arc<Self>) {
        for id in self.engine.reattachable() {
            if self.sessions.lock().unwrap().contains_key(&id) {
                continue;
            }
            if let Some(io) = self.engine.attach(&id) {
                let name = format!("Session {id}");
                self.adopt_session(id, io, name, String::new(), String::new(), Vec::new(), None);
            }
        }
    }

    /// Override the trusted snapshot-signing keys (test-only). Production always uses the hardcoded
    /// [`MODEL_CATALOG_TRUSTED_KEY`]; tests sign a snapshot with a deterministic seed and inject its
    /// public key here so the adopt/reject paths can be exercised headlessly.
    #[cfg(test)]
    pub fn with_snapshot_keys(mut self, keys: Vec<String>) -> Self {
        self.snapshot_keys = keys;
        self
    }

    /// Record how to rebuild the registry, enabling [`reload_catalog`] (CPE-375). Chained after
    /// construction by the sidecar's real wiring; tests/dev states without sources can't reload.
    pub fn with_catalog_sources(mut self, sources: CatalogSources) -> Self {
        self.sources = sources;
        self
    }

    /// Wire the host-announce hook so launched/ended sessions reach the explorer (CPE-396).
    /// Chained after construction by the sidecar; dev/tests leave the default no-op.
    pub fn with_announcer(mut self, announce: SessionAnnouncer) -> Self {
        self.announce = announce;
        self
    }

    /// Emit a session lifecycle announcement to the host (CPE-396). `event` is "started"|"ended".
    fn announce_session(&self, payload: String) {
        (self.announce)(payload);
    }

    /// Rebuild the registry from its sources and hot-swap it in (CPE-375). Returns the agent count.
    /// A no-op (returns the current count) when no sources are configured, so it can never wipe a
    /// registry that was constructed directly.
    pub fn reload_catalog(&self) -> usize {
        if self.sources.is_empty() {
            return self.registry.read().unwrap().len();
        }
        let fresh = self.sources.build();
        let count = fresh.len();
        *self.registry.write().unwrap() = fresh;
        count
    }

    fn next_id(&self) -> String {
        let mut s = self.seq.lock().unwrap();
        *s += 1;
        format!("s{}", *s)
    }

    /// The agent/provider/model/install catalog for the launcher UI. Detects install state
    /// per agent via its manifest's detect command (missing/unfound ⇒ not installed).
    fn catalog(&self) -> Value {
        let reg = self.registry.read().unwrap();
        let agents: Vec<&crate::agents::AgentManifest> = reg.all().collect();
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
                    // Reseller protocols this agent speaks (CPE-469) — the launcher offers every
                    // reseller of a matching protocol as an extra provider.
                    "resellerProtocols": a.reseller_protocols(),
                    "defaultModel": a.default_model,
                    "canInstall": a.install_for_current_os().is_some(),
                    "runnable": a.run_for_current_os().is_some(),
                })
            })
            .collect();
        // Persisted presets + remembered selection (CPE-352), for the launcher to restore.
        let presets = serde_json::to_value(self.presets.load()).unwrap_or_else(|_| json!({}));
        // Launch-capable resellers (CPE-469): the launcher offers each as a provider for agents whose
        // `resellerProtocols` include the reseller's protocol.
        let resellers: Vec<Value> = self
            .resellers
            .iter()
            .map(|r| json!({ "id": r.id, "name": r.name, "protocol": r.protocol }))
            .collect();
        json!({ "agents": agents_json, "cwd": self.default_cwd, "presets": presets, "resellers": resellers })
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

        let agent = match self.registry.read().unwrap().get(agent_id) {
            Some(a) => a.clone(),
            None => return bad(format!("unknown agent '{agent_id}'")),
        };
        // Use the provider's stored key when the user didn't type one this launch. A
        // credential label picks which of the provider's keys to use (CPE-348); default is the
        // provider's primary key (CPE-344/287).
        let credential = str_opt(&v, "credential").unwrap_or_else(|| DEFAULT_CREDENTIAL.to_string());
        let api_key = resolve_provider_key(&*self.secrets, &provider, api_key, &credential);
        // LM Studio local provider (CPE-330): auto-detect a reachable endpoint and adopt
        // its actually-loaded model, so "agent × lmstudio-local" launches with no manual
        // URL entry. Any value the caller pinned still wins; if nothing is detected the
        // recipe's `base_url` default applies. Only pay the probe cost for this provider.
        let (base_url, model) = if provider == crate::lmstudio::PROVIDER_ID {
            crate::lmstudio::resolve_launch(base_url, model, crate::lmstudio::detect_default())
        } else {
            (base_url, model)
        };
        // Capture what history/relaunch needs before `model`/`api_key` move into the launch.
        let injected_secrets: Vec<String> = api_key.iter().cloned().collect();
        let meta = SessionMeta {
            agent: agent_id.to_string(),
            provider: provider.clone(),
            model: model.clone().unwrap_or_default(),
            cwd: cwd.clone(),
            started_at: now_millis(),
        };
        let ctx = LaunchContext { model, small_model, api_key, base_url };
        // If the selected "provider" is actually a reseller gateway this build knows AND the agent
        // speaks its protocol, launch through the reseller path (CPE-469) instead of a provider recipe.
        let reseller = self
            .resellers
            .iter()
            .find(|r| r.id == provider && agent.supports_reseller(&r.protocol))
            .cloned();
        let req = AgentLaunchRequest {
            agent: &agent,
            provider: &provider,
            reseller,
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

        let id = self.next_id();
        // The session's PTY now lives wherever the engine puts it (in-process or the daemon, CPE-309).
        let io = match self.engine.launch(&id, &launch) {
            Ok(io) => io,
            Err(e) => return bad(e),
        };
        // Agent Watch announcements (CPE-396): "started" once live (below); "ended" is emitted from the
        // reader pipeline when the session's output stream closes.
        let started_payload = session_payload("started", &id, &agent.name, &meta);
        let ended_payload = session_payload("ended", &id, &agent.name, &meta);
        // Tab label for reattach (CPE-461): the launcher sends `tabName` (agent · provider · model);
        // fall back to the agent name so a session always has a label.
        let tab_name = str_opt(&v, "tabName").unwrap_or_else(|| agent.name.clone());
        self.adopt_session(
            id.clone(),
            io,
            tab_name,
            agent_id.to_string(),
            provider.clone(),
            injected_secrets,
            Some((meta, ended_payload)),
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
                // Remember which credential was used, so it's restored next open (CPE-348).
                key_ref: (credential != DEFAULT_CREDENTIAL).then(|| credential.clone()),
            },
        );
        let _ = self.presets.save(&store);
        self.announce_session(started_payload); // surface the new session to the explorer (CPE-396)

        Response::json(json!({ "session": id, "dangerousFlags": dangerous }).to_string())
    }

    /// Wire a session's I/O into the console: spawn the reader pipeline that fans the engine's output
    /// channel into the replay ring + the live WebSocket, taps reads (CPE-405) + usage (CPE-311), and
    /// on stream-close records history + announces "ended"; then insert the `Session`. Shared by a
    /// fresh `handle_launch` and boot-time `reattach_running_sessions` (CPE-309). `end` is
    /// `Some((meta, ended_payload))` for a launch (full history record) or `None` for a reattached
    /// session (identity was console-side and didn't survive the restart — a minimal `ended` announce
    /// is emitted by id instead).
    #[allow(clippy::too_many_arguments)]
    fn adopt_session(
        &self,
        id: String,
        io: Arc<dyn crate::session_engine::SessionIo>,
        name: String,
        agent: String,
        provider: String,
        injected_secrets: Vec<String>,
        end: Option<(SessionMeta, String)>,
    ) {
        let ring: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let live: Arc<Mutex<Option<mpsc::Sender<Vec<u8>>>>> = Arc::new(Mutex::new(None));
        let usage: Arc<Mutex<crate::usage::Usage>> = Arc::new(Mutex::new(crate::usage::Usage::default()));
        let out = io.take_output();
        {
            let ring = Arc::clone(&ring);
            let live = Arc::clone(&live);
            let usage = Arc::clone(&usage);
            let history = Arc::clone(&self.history);
            let announce = Arc::clone(&self.announce);
            let record_id = id.clone();
            let read_cwd = end.as_ref().map(|(m, _)| m.cwd.clone()).unwrap_or_default();
            thread::spawn(move || {
                let Some(rx) = out else { return };
                let mut reads = crate::agent_reads::ReadScanner::new();
                let mut usage_scan = crate::usage::UsageScanner::new();
                while let Ok(chunk) = rx.recv() {
                    {
                        let mut r = ring.lock().unwrap();
                        r.extend_from_slice(&chunk);
                        let over = r.len().saturating_sub(RING_CAP);
                        if over > 0 {
                            r.drain(0..over);
                        }
                    }
                    // Push live to the attached terminal, if any (drop on send error).
                    if let Some(tx) = live.lock().unwrap().as_ref() {
                        let _ = tx.send(chunk.clone());
                    }
                    let text = String::from_utf8_lossy(&chunk);
                    // Surface any file the agent reported reading (CPE-405).
                    for raw in reads.feed(&text) {
                        announce(read_announcement(&read_cwd, &raw));
                    }
                    // Fold provider-reported token/cost usage for this session (CPE-311).
                    *usage.lock().unwrap() = usage_scan.feed(&text);
                }
                // Stream closed → the agent exited (or the daemon connection dropped).
                match end {
                    Some((meta, ended_payload)) => {
                        let transcript = String::from_utf8_lossy(&ring.lock().unwrap()).into_owned();
                        record_session_end(&*history, &meta, &record_id, transcript, &injected_secrets);
                        announce(ended_payload); // tell the explorer this session is gone (CPE-396)
                    }
                    None => announce(format!(r#"{{"event":"ended","sessionId":"{record_id}"}}"#)),
                }
            });
        }
        self.sessions.lock().unwrap().insert(
            id,
            Arc::new(Session { io, ring, live, name, usage, agent, provider }),
        );
    }

    /// Close one session: remove it from the live set and kill its agent PTY. Killing the child
    /// drives the session's reader pipeline to EOF, which runs the normal end path (history record +
    /// the `ended` announce for Agent Watch), so no separate teardown bookkeeping is needed. Returns
    /// whether a session by that id was present. Idempotent: closing an unknown id is a no-op.
    fn close_session(&self, id: &str) -> bool {
        let removed = self.sessions.lock().unwrap().remove(id);
        match removed {
            Some(s) => {
                let _ = s.io.kill();
                // Announce the end immediately so the explorer's Agents leaf disappears now, rather
                // than waiting for the reader thread's EOF (which a Windows ConPTY may withhold).
                self.announce_ended(id);
                true
            }
            None => false,
        }
    }

    /// Minimal `ended` announcement (just the id) so a closed session's Agent-Watch leaf is removed
    /// promptly. The frontend reducer keys removal on `sessionId` alone (CPE-397).
    fn announce_ended(&self, id: &str) {
        self.announce_session(format!(r#"{{"event":"ended","sessionId":"{id}"}}"#));
    }

    /// Close **every** live session at once (CPE-442) — the fan-out teardown behind the launcher's
    /// "Close all" and process shutdown. Kills each agent PTY (reclaiming the child process + its
    /// PTY) and empties the set, so nothing is left running. Idempotent: with nothing open it returns
    /// an empty list. Returns the closed ids, sorted.
    pub fn close_all(&self) -> Vec<String> {
        let drained: Vec<(String, Arc<Session>)> = self.sessions.lock().unwrap().drain().collect();
        let mut ids: Vec<String> = Vec::with_capacity(drained.len());
        for (id, s) in drained {
            let _ = s.io.kill();
            self.announce_ended(&id); // remove each Agents leaf immediately (CPE-397)
            ids.push(id);
        }
        ids.sort();
        ids
    }

    /// `POST /api/session/{id}/close` → close one session and reclaim its process/PTY.
    fn handle_close(&self, path: &str) -> Response {
        let id = session_id_from(path, "/close");
        Response::json(json!({ "closed": self.close_session(&id) }).to_string())
    }

    /// `POST /api/close-all` → close every session and reclaim all out-of-process resources.
    fn handle_close_all(&self) -> Response {
        Response::json(json!({ "closed": self.close_all() }).to_string())
    }

    /// `GET /api/sessions` → the running sessions `[{id, name}]`, so the launcher can **reattach** a
    /// tab to each still-running session when the AI Console is closed and reopened (CPE-461). The
    /// sessions live in this process independent of the (destroyed-on-close) launcher UI.
    fn handle_sessions_list(&self) -> Response {
        let sessions = self.sessions.lock().unwrap();
        // Snapshot id, name, identity, and usage under the lock (CPE-461 + CPE-311).
        let mut rows: Vec<(String, String, String, String, crate::usage::Usage)> = sessions
            .iter()
            .map(|(id, s)| {
                (id.clone(), s.name.clone(), s.agent.clone(), s.provider.clone(), *s.usage.lock().unwrap())
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0)); // sequential ids ⇒ launch order

        // Aggregate usage per agent and per provider (CPE-311).
        let mut by_agent: BTreeMap<String, crate::usage::Usage> = BTreeMap::new();
        let mut by_provider: BTreeMap<String, crate::usage::Usage> = BTreeMap::new();
        for (_, _, agent, provider, u) in &rows {
            accumulate_usage(by_agent.entry(agent.clone()).or_default(), u);
            accumulate_usage(by_provider.entry(provider.clone()).or_default(), u);
        }

        let out: Vec<Value> = rows
            .into_iter()
            .map(|(id, name, _, _, u)| {
                json!({ "id": id, "name": name, "usage": usage_json(&u) })
            })
            .collect();
        let agents: Vec<Value> =
            by_agent.into_iter().map(|(k, u)| json!({ "id": k, "usage": usage_json(&u) })).collect();
        let providers: Vec<Value> =
            by_provider.into_iter().map(|(k, u)| json!({ "id": k, "usage": usage_json(&u) })).collect();
        Response::json(
            json!({ "sessions": out, "usageByAgent": agents, "usageByProvider": providers }).to_string(),
        )
    }

    fn handle_resize(&self, path: &str, body: &str) -> Response {
        let id = session_id_from(path, "/resize");
        let v: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({}));
        let rows = v["rows"].as_u64().unwrap_or(30) as u16;
        let cols = v["cols"].as_u64().unwrap_or(100) as u16;
        let sess = {
            let sessions = self.sessions.lock().unwrap();
            match sessions.get(&id) {
                Some(s) => Arc::clone(s),
                None => return bad("no such session"),
            }
        };
        match sess.io.resize(rows, cols) {
            Ok(()) => Response::json("{\"ok\":true}"),
            Err(e) => bad(e),
        }
    }

    /// Download, verify, and adopt the published model-catalog snapshot (CPE-451). Asks the host to
    /// fetch `models-index.json` + `.sig`, parses the index as a [`ModelSnapshot`], checks the
    /// ed25519 signature against [`Self::snapshot_keys`] and the strictly-monotonic anti-rollback
    /// counter against the cached version, and on success caches `(version, models)` as the picker's
    /// default source. **Fail-safe:** any failure — no host, malformed JSON, unverifiable signature,
    /// or an older/equal version — leaves the cache untouched, so the picker falls back to the live
    /// per-reseller fetch. Never errors; returns whether a new snapshot was adopted.
    pub fn refresh_snapshot(&self) -> bool {
        let (index, sig) = match self.dialogs.fetch_model_snapshot() {
            Ok(v) => v,
            Err(_) => return false, // no host / fetch failed → keep whatever we had
        };
        let snap: crate::model_snapshot::ModelSnapshot = match serde_json::from_str(&index) {
            Ok(s) => s,
            Err(_) => return false, // malformed snapshot is ignored, never adopted
        };
        // Fail-closed signature check, then strict anti-rollback against the cached version.
        if !crate::model_snapshot::verify_snapshot(&snap, &sig, &self.snapshot_keys) {
            return false;
        }
        let current = self.snapshot_models.lock().unwrap().as_ref().map(|(v, _)| *v);
        if !crate::model_snapshot::accept_snapshot(current, &snap) {
            return false;
        }
        *self.snapshot_models.lock().unwrap() = Some((snap.version, snap.models));
        true
    }

    /// `GET /api/models?reseller=<id>` → the model list for the picker (CPE-449/451). Prefers the
    /// **downloaded, verified snapshot** (fast + offline) for a reseller it covers, populating it
    /// lazily on first use; falls back to the live per-reseller fetch via the host's allow-listed
    /// egress otherwise. `?refresh=1` forces the live path (a manual Refresh). Defaults to
    /// `openrouter`, whose list is public (no key). Returns `{ reseller, models, source }`, or an
    /// error the picker shows inline.
    fn handle_models(&self, path: &str) -> Response {
        let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
        let reseller = query
            .split('&')
            .find_map(|kv| kv.strip_prefix("reseller="))
            .filter(|r| !r.is_empty())
            .unwrap_or("openrouter");
        let force_refresh = query.split('&').any(|kv| kv == "refresh=1");

        // Prefer the verified snapshot unless the caller forced the live path. Populate it lazily on
        // first use so the download happens on demand (a background refresh is a fine future add).
        if !force_refresh {
            if self.snapshot_models.lock().unwrap().is_none() {
                self.refresh_snapshot();
            }
            if let Some((version, models)) = self.snapshot_models.lock().unwrap().clone() {
                let models: Vec<_> = models.into_iter().filter(|m| m.reseller == reseller).collect();
                // Only serve from the snapshot if it actually covers this reseller; otherwise fall
                // through to the live fetch below.
                if !models.is_empty() {
                    return Response::json(json!({
                        "reseller": reseller,
                        "models": models,
                        "source": "snapshot",
                        "snapshotVersion": version,
                    }).to_string());
                }
            }
        }

        // Live per-reseller fetch — the fallback and the forced-refresh path (CPE-449).
        // A per-reseller key is used when present (CPE-452), but listing usually needs none.
        let token = self.secrets.get(&reseller_secret_name(reseller, DEFAULT_CREDENTIAL)).ok().flatten();
        match self.dialogs.list_models(reseller, token.as_deref()) {
            Ok(models) => Response::json(json!({ "reseller": reseller, "models": models, "source": "live" }).to_string()),
            Err(e) => bad(e),
        }
    }

    fn handle_install(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let agent_id = v["agent"].as_str().unwrap_or("");
        let agent = match self.registry.read().unwrap().get(agent_id) {
            Some(a) => a.clone(),
            None => return bad(format!("unknown agent '{agent_id}'")),
        };
        match lifecycle::install(&agent, &RealRunner) {
            Ok(out) => Response::json(json!({ "ok": true, "output": out.stdout }).to_string()),
            Err(e) => bad(e),
        }
    }

    /// Open the host's native folder picker for the Working-folder box (CPE-354). Returns
    /// `{ path }` (null when cancelled). The sandboxed launcher can't open dialogs itself.
    fn handle_pick_folder(&self, body: &str) -> Response {
        // The launcher passes the Project-folder box's current value so the picker opens there.
        let start = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|v| v.get("start").and_then(Value::as_str).map(str::to_string))
            .filter(|s| !s.is_empty());
        match self.dialogs.pick_folder(start.as_deref()) {
            Ok(path) => Response::json(json!({ "path": path }).to_string()),
            Err(e) => bad(e),
        }
    }

    /// Mark first-run onboarding as dismissed so it doesn't show again (CPE-312).
    fn handle_onboarded(&self) -> Response {
        let mut store = self.presets.load();
        store.onboarded = true;
        match self.presets.save(&store) {
            Ok(()) => Response::json(json!({ "ok": true }).to_string()),
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

    /// The credential label from a request body, defaulting to the provider's primary key.
    fn credential_label(v: &Value) -> String {
        let l = v["label"].as_str().unwrap_or("").trim();
        if l.is_empty() { DEFAULT_CREDENTIAL.to_string() } else { l.to_string() }
    }

    /// Store a provider's API key in the OS keychain (via the broker), under an optional
    /// label so a provider can hold several credentials (CPE-345/348). Format-checked first.
    fn handle_key_set(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let provider = v["provider"].as_str().unwrap_or("").trim();
        let key = v["key"].as_str().unwrap_or("").trim();
        let label = Self::credential_label(&v);
        if provider.is_empty() {
            return bad("missing 'provider'");
        }
        if let Err(e) = crate::keycheck::check_key_format(provider, key) {
            return bad(e);
        }
        if let Err(e) = self.secrets.set(&provider_secret_name(provider, &label), key) {
            return bad(e);
        }
        // Track the credential's identity so labelled keys are listable (CPE-348).
        let mut store = self.presets.load();
        store.add_credential(provider, &label);
        let _ = self.presets.save(&store);
        Response::json(json!({ "ok": true }).to_string())
    }

    /// List stored credentials as `{provider, label}` — NAMES ONLY, never a value (CPE-348).
    /// Merges the persisted index (labelled keys) with a probe for legacy default keys.
    fn handle_key_list(&self) -> Response {
        let store = self.presets.load();
        let mut creds: Vec<crate::presets::CredentialRef> = store.credentials.clone();
        // Back-compat: default keys stored before the index existed are found by probing.
        let mut providers: Vec<String> =
            self.registry.read().unwrap().all().flat_map(|a| a.providers.iter().cloned()).collect();
        providers.sort();
        providers.dedup();
        for p in providers {
            if self.secrets.get(&provider_secret_name(&p, DEFAULT_CREDENTIAL)).ok().flatten().is_some()
                && !creds.iter().any(|c| c.provider == p && c.label == DEFAULT_CREDENTIAL)
            {
                creds.push(crate::presets::CredentialRef { provider: p, label: DEFAULT_CREDENTIAL.into() });
            }
        }
        // Legacy field `providers` (names of anything with a key) kept for compatibility.
        let mut names: Vec<String> = creds.iter().map(|c| c.provider.clone()).collect();
        names.sort();
        names.dedup();
        Response::json(json!({ "credentials": creds, "providers": names }).to_string())
    }

    /// Remove a provider's stored key (optionally a specific label) and drop it from the index.
    fn handle_key_delete(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let provider = v["provider"].as_str().unwrap_or("").trim();
        let label = Self::credential_label(&v);
        if provider.is_empty() {
            return bad("missing 'provider'");
        }
        if let Err(e) = self.secrets.delete(&provider_secret_name(provider, &label)) {
            return bad(e);
        }
        let mut store = self.presets.load();
        store.remove_credential(provider, &label);
        let _ = self.presets.save(&store);
        Response::json(json!({ "ok": true }).to_string())
    }

    /// `POST /api/reseller-keys {reseller, key, label?}` — store a model reseller's API key in the
    /// keychain namespace (CPE-452), so the picker + inference can authenticate. Kept separate from
    /// provider keys; no provider-specific format check (resellers vary), just non-empty.
    fn handle_reseller_key_set(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let reseller = v["reseller"].as_str().unwrap_or("").trim();
        let key = v["key"].as_str().unwrap_or("").trim();
        let label = Self::credential_label(&v);
        if reseller.is_empty() {
            return bad("missing 'reseller'");
        }
        if key.is_empty() {
            return bad("missing 'key'");
        }
        if let Err(e) = self.secrets.set(&reseller_secret_name(reseller, &label), key) {
            return bad(e);
        }
        Response::json(json!({ "ok": true }).to_string())
    }

    /// The model resellers the Keys panel offers a key entry for (CPE-452) — matches the bundled
    /// `resellers/*.json`. Kept here so listing can probe for stored keys without enumerating the
    /// keychain (which the broker can't do).
    const KNOWN_RESELLERS: &[&str] = &[
        "openrouter", "together", "fireworks", "groq", "deepinfra", "novita", "aimlapi",
        "wavespeed", "github-models", "cerebras", "sambanova", "nebius", "hyperbolic",
        "mistral", "deepseek", "cohere", "requesty", "glama", "vercel",
    ];

    /// `GET /api/reseller-keys` → the resellers that have a stored key (names only, never a value).
    /// Probes each known reseller's default slot, mirroring how `handle_key_list` finds legacy keys.
    fn handle_reseller_key_list(&self) -> Response {
        let stored: Vec<Value> = Self::KNOWN_RESELLERS
            .iter()
            .filter(|r| self.secrets.get(&reseller_secret_name(r, DEFAULT_CREDENTIAL)).ok().flatten().is_some())
            .map(|r| json!({ "reseller": r }))
            .collect();
        Response::json(json!({ "resellers": stored }).to_string())
    }

    /// `POST /api/reseller-keys/delete {reseller, label?}` — remove a reseller key.
    fn handle_reseller_key_delete(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let reseller = v["reseller"].as_str().unwrap_or("").trim();
        let label = Self::credential_label(&v);
        if reseller.is_empty() {
            return bad("missing 'reseller'");
        }
        if let Err(e) = self.secrets.delete(&reseller_secret_name(reseller, &label)) {
            return bad(e);
        }
        Response::json(json!({ "ok": true }).to_string())
    }

    /// Check a key before saving (CPE-345/347): the cheap offline shape check first, then — if the
    /// shape is plausible — a live check against the provider via the host. `live:true` in the
    /// reply means the provider gave a definitive answer; `live:false` is the offline "format
    /// looks valid" result (no verifier for this provider, or the live check couldn't run).
    fn handle_key_verify(&self, body: &str) -> Response {
        let v: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return bad(format!("bad json: {e}")),
        };
        let provider = v["provider"].as_str().unwrap_or("").trim();
        let key = v["key"].as_str().unwrap_or("");
        // Never spend a network round-trip on an obviously-malformed key.
        if let Err(e) = crate::keycheck::check_key_format(provider, key) {
            return Response::json(json!({ "valid": false, "live": false, "detail": e }).to_string());
        }
        match self.dialogs.verify_key(provider, key) {
            // Definitive provider answer (verified or rejected) — pass it straight through.
            Ok(verdict) if verdict.live => Response::json(
                json!({ "valid": verdict.valid, "live": true, "detail": verdict.detail }).to_string(),
            ),
            // No live verifier for this provider — report the offline result, using the host's
            // explanation when it gave one.
            Ok(verdict) => {
                let detail =
                    if verdict.detail.is_empty() { "Format looks valid.".into() } else { verdict.detail };
                Response::json(json!({ "valid": true, "live": false, "detail": detail }).to_string())
            }
            // The live check couldn't run (dev/standalone, or a transient host error) — don't
            // block the user; fall back to the offline result.
            Err(_) => Response::json(
                json!({ "valid": true, "live": false, "detail": "Format looks valid (live check unavailable)." })
                    .to_string(),
            ),
        }
    }

    /// `GET /api/history` → recent sessions (newest first), metadata only — no transcript, so the
    /// list stays light. Powers the launcher's "Recent sessions" panel (CPE-370).
    fn handle_history_list(&self) -> Response {
        let hist = self.history.load();
        let sessions: Vec<Value> = hist
            .recent()
            .map(|s| {
                json!({
                    "id": s.id, "agent": s.agent, "provider": s.provider,
                    "model": s.model, "cwd": s.cwd, "startedAt": s.started_at,
                })
            })
            .collect();
        Response::json(json!({ "sessions": sessions }).to_string())
    }

    /// `POST /api/catalog/reload` → re-scan the catalog sources and hot-swap the registry after an
    /// update was applied to disk (CPE-375). Returns the new agent count.
    fn handle_catalog_reload(&self) -> Response {
        Response::json(json!({ "agents": self.reload_catalog() }).to_string())
    }

    /// `POST /api/catalog/refresh` → ask the host to fetch + apply the signed catalog bundle
    /// (CPE-376), then hot-reload if anything changed. Returns `{ indexOk, applied, agents }`.
    fn handle_catalog_refresh(&self) -> Response {
        let pinned = self.presets.load().pinned_agents;
        match self.dialogs.fetch_catalog(&pinned) {
            Ok(res) => {
                let agents = if res.applied > 0 {
                    self.reload_catalog()
                } else {
                    self.registry.read().unwrap().len()
                };
                Response::json(
                    json!({ "indexOk": res.index_ok, "applied": res.applied, "agents": agents })
                        .to_string(),
                )
            }
            Err(e) => bad(e),
        }
    }

    /// `POST /api/catalog/reset` → roll back to the **shipped** catalog (CPE-379): clear the fetched
    /// verified source (manifests + sigs + the version map) and hot-reload, so the registry returns
    /// to the bundled first-party agents. The simplest, safest rollback — undo a bad update.
    fn handle_catalog_reset(&self) -> Response {
        if let Some(dir) = &self.sources.signed_dir {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for path in entries.flatten().map(|e| e.path()) {
                    let clear = path
                        .extension()
                        .map(|x| x == "json" || x == "sig")
                        .unwrap_or(false);
                    if clear {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
        }
        Response::json(json!({ "reset": true, "agents": self.reload_catalog() }).to_string())
    }

    /// `POST /api/catalog/settings {autoUpdate}` → persist the opt-in auto-update flag (CPE-378).
    fn handle_catalog_settings(&self, body: &str) -> Response {
        let v: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({}));
        let Some(on) = v["autoUpdate"].as_bool() else { return bad("autoUpdate must be a bool") };
        let mut store = self.presets.load();
        store.auto_update_catalog = on;
        match self.presets.save(&store) {
            Ok(()) => Response::json(json!({ "autoUpdate": on }).to_string()),
            Err(e) => bad(e),
        }
    }

    /// `POST /api/catalog/pin {agent, pinned}` → pin/unpin an agent from catalog updates (CPE-378).
    fn handle_catalog_pin(&self, body: &str) -> Response {
        let v: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({}));
        let agent = v["agent"].as_str().unwrap_or("").trim();
        if agent.is_empty() {
            return bad("agent is required");
        }
        let pinned = v["pinned"].as_bool().unwrap_or(true);
        let mut store = self.presets.load();
        store.set_pinned(agent, pinned);
        match self.presets.save(&store) {
            Ok(()) => Response::json(json!({ "agent": agent, "pinned": pinned }).to_string()),
            Err(e) => bad(e),
        }
    }

    /// `GET /api/catalog/versions` → enumerate prior published catalog versions for the rollback
    /// picker (CPE-383). Host-mediated GitHub Releases API; empty list when there's no host/offline.
    fn handle_catalog_versions(&self) -> Response {
        match self.dialogs.list_catalog_versions() {
            Ok(vs) => {
                let arr: Vec<Value> = vs
                    .into_iter()
                    .map(|v| json!({ "tag": v.tag, "publishedAt": v.published_at, "prerelease": v.prerelease }))
                    .collect();
                Response::json(json!({ "versions": arr }).to_string())
            }
            // No host (dev/standalone) is not an error to the UI — just an empty picker.
            Err(_) => Response::json(json!({ "versions": [] }).to_string()),
        }
    }

    /// `POST /api/catalog/rollback {tag, agents}` → roll the chosen `agents` back to a specific prior
    /// published version `tag` (CPE-383): an audited per-agent downgrade override. Hot-reloads on
    /// apply. `agents` empty ⇒ nothing to do.
    fn handle_catalog_rollback(&self, body: &str) -> Response {
        let v: Value = serde_json::from_str(body).unwrap_or_else(|_| json!({}));
        let tag = v["tag"].as_str().unwrap_or("").trim().to_string();
        if tag.is_empty() {
            return bad("tag is required");
        }
        let agents: Vec<String> = v["agents"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if agents.is_empty() {
            return bad("at least one agent is required to roll back");
        }
        match self.dialogs.rollback_catalog(&tag, &agents) {
            Ok(res) => {
                let agents_now = if res.applied > 0 {
                    self.reload_catalog()
                } else {
                    self.registry.read().unwrap().len()
                };
                Response::json(
                    json!({ "indexOk": res.index_ok, "applied": res.applied, "tag": tag, "agents": agents_now })
                        .to_string(),
                )
            }
            Err(e) => bad(e),
        }
    }

    /// `GET /api/history/{id}` → one session's stored (already-redacted) transcript + metadata.
    fn handle_history_detail(&self, path: &str) -> Response {
        let id = path.strip_prefix("/api/history/").unwrap_or("");
        let hist = self.history.load();
        let found = hist.recent().find(|s| s.id == id).cloned();
        match found {
            Some(s) => Response::json(
                json!({
                    "id": s.id, "agent": s.agent, "provider": s.provider, "model": s.model,
                    "cwd": s.cwd, "startedAt": s.started_at, "transcript": s.transcript,
                })
                .to_string(),
            ),
            None => bad("no such session"),
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

/// Epoch-millis as a string, used as a session's start time (opaque to history; the launcher
/// formats it). Empty on the impossible pre-epoch clock.
fn now_millis() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_default()
}

/// Append a finished session to history and persist it — best-effort, never panics the reader
/// thread. The transcript is redacted against the injected key(s) before storage (CPE-370/292).
fn record_session_end(
    history: &dyn crate::history::HistoryBackend,
    meta: &SessionMeta,
    id: &str,
    transcript: String,
    secrets: &[String],
) {
    let record = crate::history::SessionRecord {
        id: id.to_string(),
        agent: meta.agent.clone(),
        provider: meta.provider.clone(),
        model: (!meta.model.is_empty()).then(|| meta.model.clone()),
        cwd: meta.cwd.clone(),
        started_at: meta.started_at.clone(),
        transcript,
    };
    let mut hist = history.load();
    hist.record(record, secrets);
    let _ = history.save(&hist);
}

/// Route one request to the console API or the launcher page.
pub fn route(state: &ConsoleState, req: &Request) -> Response {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => Response::html(launcher_html()),
        ("GET", "/api/catalog") => Response::json(state.catalog().to_string()),
        ("POST", "/api/launch") => state.handle_launch(&req.body),
        ("POST", "/api/close-all") => state.handle_close_all(),
        ("POST", p) if p.starts_with("/api/session/") && p.ends_with("/close") => {
            state.handle_close(p)
        }
        ("POST", "/api/install") => state.handle_install(&req.body),
        ("POST", "/api/presets") => state.handle_preset_save(&req.body),
        ("POST", "/api/presets/delete") => state.handle_preset_delete(&req.body),
        ("POST", "/api/onboarded") => state.handle_onboarded(),
        ("POST", "/api/pick-folder") => state.handle_pick_folder(&req.body),
        ("GET", "/api/sessions") => state.handle_sessions_list(),
        ("GET", p) if p.starts_with("/api/models") => state.handle_models(p),
        ("GET", "/api/keys") => state.handle_key_list(),
        ("POST", "/api/keys") => state.handle_key_set(&req.body),
        ("POST", "/api/keys/delete") => state.handle_key_delete(&req.body),
        ("GET", "/api/reseller-keys") => state.handle_reseller_key_list(),
        ("POST", "/api/reseller-keys") => state.handle_reseller_key_set(&req.body),
        ("POST", "/api/reseller-keys/delete") => state.handle_reseller_key_delete(&req.body),
        ("POST", "/api/keys/verify") => state.handle_key_verify(&req.body),
        ("POST", "/api/catalog/reload") => state.handle_catalog_reload(),
        ("POST", "/api/catalog/refresh") => state.handle_catalog_refresh(),
        ("POST", "/api/catalog/reset") => state.handle_catalog_reset(),
        ("GET", "/api/catalog/versions") => state.handle_catalog_versions(),
        ("POST", "/api/catalog/rollback") => state.handle_catalog_rollback(&req.body),
        ("POST", "/api/catalog/settings") => state.handle_catalog_settings(&req.body),
        ("POST", "/api/catalog/pin") => state.handle_catalog_pin(&req.body),
        ("GET", "/api/history") => state.handle_history_list(),
        ("GET", p) if p.starts_with("/api/history/") => state.handle_history_detail(p),
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
                let _ = sess.io.write(&frame.payload);
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
    fn session_payload_carries_identity_and_cwd_without_secrets() {
        // Agent Watch (CPE-396): the announcement must locate the session (cwd = Project folder)
        // and never leak a key.
        let meta = SessionMeta {
            agent: "claude".into(),
            provider: "openrouter".into(),
            model: "sonnet".into(),
            cwd: "Z:/repos/app".into(),
            started_at: "0".into(),
        };
        let v: Value = serde_json::from_str(&session_payload("started", "s1", "Claude Code", &meta)).unwrap();
        assert_eq!(v["event"], "started");
        assert_eq!(v["sessionId"], "s1");
        assert_eq!(v["agentId"], "claude");
        assert_eq!(v["agentName"], "Claude Code");
        assert_eq!(v["provider"], "openrouter");
        assert_eq!(v["cwd"], "Z:/repos/app");
        assert!(v.get("apiKey").is_none() && v.get("key").is_none() && v.get("secret").is_none());
    }

    #[test]
    fn announcer_hook_receives_what_the_console_emits() {
        use std::sync::{Arc, Mutex};
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = seen.clone();
        let state = state().with_announcer(Arc::new(move |p| sink.lock().unwrap().push(p)));
        state.announce_session("session:{\"event\":\"ended\"}".into());
        assert_eq!(seen.lock().unwrap().as_slice(), &["session:{\"event\":\"ended\"}".to_string()]);
    }

    #[test]
    fn read_announcement_resolves_against_cwd_and_tags_the_channel() {
        // A relative read path is resolved against the session's Project folder, so it matches the
        // absolute paths the FS watcher emits; the payload is a `fs-read:<json>` the host forwards.
        let s = read_announcement("Z:/repos/app", "src/main.rs");
        let json = s.strip_prefix("fs-read:").expect("fs-read prefix");
        let v: Value = serde_json::from_str(json).unwrap();
        let path = v["path"].as_str().unwrap().replace('\\', "/");
        assert_eq!(path, "Z:/repos/app/src/main.rs");
        // An already-absolute path is preserved (join replaces the base with an absolute child).
        let abs = if cfg!(windows) { "C:/tmp/x.rs" } else { "/tmp/x.rs" };
        let s2 = read_announcement("Z:/repos/app", abs);
        let v2: Value = serde_json::from_str(s2.strip_prefix("fs-read:").unwrap()).unwrap();
        assert_eq!(v2["path"].as_str().unwrap().replace('\\', "/"), abs);
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
        assert_eq!(resolve_provider_key(&s, "openrouter", None, "default"), None);
        // A stored default key is used when the user didn't type one…
        s.set(&provider_secret_name("openrouter", "default"), "sk-stored").unwrap();
        assert_eq!(resolve_provider_key(&s, "openrouter", None, "default").as_deref(), Some("sk-stored"));
        // …but an explicit key always wins…
        assert_eq!(
            resolve_provider_key(&s, "openrouter", Some("sk-typed".into()), "default").as_deref(),
            Some("sk-typed"),
        );
        // …a different provider isn't affected…
        assert_eq!(resolve_provider_key(&s, "anthropic", None, "default"), None);
        // …and a labelled credential is a distinct key (CPE-348).
        s.set(&provider_secret_name("openrouter", "work"), "sk-work").unwrap();
        assert_eq!(resolve_provider_key(&s, "openrouter", None, "work").as_deref(), Some("sk-work"));
        assert_eq!(resolve_provider_key(&s, "openrouter", None, "default").as_deref(), Some("sk-stored"));
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

        // With no host verifier (NoopDialogs) verify falls back to the offline shape check
        // (live:false), never blocking on a check it can't run.
        let v: Value = serde_json::from_slice(&post("/api/keys/verify", r#"{"provider":"openrouter","key":"sk-or-abcdef123456"}"#).body).unwrap();
        assert_eq!(v["valid"], true);
        assert_eq!(v["live"], false);

        // Delete removes it from the list.
        assert_eq!(post("/api/keys/delete", r#"{"provider":"openrouter"}"#).status, 200);
        assert!(!String::from_utf8_lossy(&get(&st, "/api/keys").body).contains("openrouter"));
    }

    #[test]
    fn a_key_saved_via_the_keys_panel_is_the_one_the_launch_resolver_uses() {
        // The end-to-end contract behind "the GUI can use this key": a key stored via the Keys
        // panel API (handle_key_set) must resolve, under the same provider_secret_name, to exactly
        // what the launch flow (resolve_provider_key, used by handle_launch → the agent env) reads.
        // Fake keys only — never a real one.
        let st = state();
        let post = |path: &str, body: &str| {
            route(&st, &Request { method: "POST".into(), path: path.into(), body: body.into(), ..Default::default() })
        };
        // Save a default key and a labelled ("work") key for openrouter via the Keys panel.
        assert_eq!(post("/api/keys", r#"{"provider":"openrouter","key":"sk-or-defaultkey000"}"#).status, 200);
        assert_eq!(
            post("/api/keys", r#"{"provider":"openrouter","key":"sk-or-workkey00000","label":"work"}"#).status,
            200
        );

        // The launch resolver picks up exactly what the API stored — for the default and the label.
        assert_eq!(
            resolve_provider_key(&*st.secrets, "openrouter", None, "default").as_deref(),
            Some("sk-or-defaultkey000")
        );
        assert_eq!(
            resolve_provider_key(&*st.secrets, "openrouter", None, "work").as_deref(),
            Some("sk-or-workkey00000")
        );
        // A key typed for this one launch overrides the stored one (precedence, CPE-344/348).
        assert_eq!(
            resolve_provider_key(&*st.secrets, "openrouter", Some("sk-or-typednow0000".into()), "default").as_deref(),
            Some("sk-or-typednow0000")
        );

        // Deleting the default via the API → the launch resolves to no key (falls back to the
        // agent's native login); the labelled key is untouched.
        assert_eq!(post("/api/keys/delete", r#"{"provider":"openrouter"}"#).status, 200);
        assert_eq!(resolve_provider_key(&*st.secrets, "openrouter", None, "default"), None);
        assert_eq!(
            resolve_provider_key(&*st.secrets, "openrouter", None, "work").as_deref(),
            Some("sk-or-workkey00000")
        );
    }

    /// A [`HostDialogs`] whose `verify_key` returns a scripted verdict and records whether it was
    /// called — to prove the live path is taken (and skipped for bad-format keys). CPE-347.
    struct StubDialogs {
        verdict: crate::broker_client::KeyVerdict,
        called: std::sync::atomic::AtomicBool,
    }
    impl crate::broker_client::HostDialogs for StubDialogs {
        fn pick_folder(&self, _start: Option<&str>) -> Result<Option<String>, String> {
            Ok(None)
        }
        fn verify_key(&self, _p: &str, _k: &str) -> Result<crate::broker_client::KeyVerdict, String> {
            self.called.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(self.verdict.clone())
        }
        fn fetch_catalog(&self, _pinned: &[String]) -> Result<crate::broker_client::CatalogFetch, String> {
            Err("stub".into())
        }
        fn list_models(&self, reseller: &str, _token: Option<&str>) -> Result<Vec<crate::model_catalog::Model>, String> {
            // Two canned OpenRouter-shaped models so the /api/models route has something to return.
            let json = r#"{"data":[
                {"id":"anthropic/claude-3.5-sonnet","name":"Claude 3.5 Sonnet","context_length":200000,"pricing":{"prompt":"0.000003","completion":"0.000015"}},
                {"id":"openai/gpt-4o","name":"GPT-4o","context_length":128000,"pricing":{"prompt":"0.0000025","completion":"0.00001"}}
            ]}"#;
            Ok(crate::model_catalog::normalize_models(reseller, json))
        }
        fn list_catalog_versions(
            &self,
        ) -> Result<Vec<crate::broker_client::CatalogVersion>, String> {
            Ok(vec![crate::broker_client::CatalogVersion {
                tag: "v0.1.0".into(),
                published_at: "2026-07-01T00:00:00Z".into(),
                prerelease: false,
            }])
        }
        fn rollback_catalog(
            &self,
            _tag: &str,
            _agents: &[String],
        ) -> Result<crate::broker_client::CatalogFetch, String> {
            Ok(crate::broker_client::CatalogFetch { index_ok: true, applied: 1 })
        }
        fn fetch_model_snapshot(&self) -> Result<(String, String), String> {
            // No snapshot from this stub → /api/models falls back to the live `list_models` above.
            Err("stub: no snapshot".into())
        }
    }

    fn state_with_dialogs(dialogs: Arc<StubDialogs>) -> ConsoleState {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents");
        let registry = AgentRegistry::load_from_dirs(&[dir]);
        ConsoleState::with_backends(
            registry,
            "/repo".into(),
            Arc::new(crate::broker_client::MemSecrets::default()),
            Arc::new(crate::presets::MemPresets::default()),
            dialogs,
            Arc::new(crate::history::MemHistory::default()),
        )
    }

    #[test]
    fn models_route_returns_the_resellers_normalized_list() {
        let dialogs = Arc::new(StubDialogs {
            verdict: crate::broker_client::KeyVerdict { valid: true, live: true, detail: String::new() },
            called: std::sync::atomic::AtomicBool::new(false),
        });
        let st = state_with_dialogs(dialogs);
        let r = route(&st, &Request { method: "GET".into(), path: "/api/models?reseller=openrouter".into(), ..Default::default() });
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["reseller"], "openrouter");
        let models = v["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["id"], "anthropic/claude-3.5-sonnet");
        assert_eq!(models[0]["context_length"], 200000);
        // No snapshot available (StubDialogs returns Err) → the live per-reseller path is served.
        assert_eq!(v["source"], "live");
        // Default reseller when the query is absent.
        let r2 = route(&st, &Request { method: "GET".into(), path: "/api/models".into(), ..Default::default() });
        assert_eq!(serde_json::from_slice::<Value>(&r2.body).unwrap()["reseller"], "openrouter");
    }

    /// A [`HostDialogs`] that serves a scripted, swappable model snapshot (index + sig) and records
    /// whether the **live** `list_models` fallback was hit — to prove the picker prefers a verified
    /// snapshot and only falls back when it can't be adopted (CPE-451).
    struct SnapshotDialogs {
        snapshot: Mutex<Option<(String, String)>>,
        live_called: std::sync::atomic::AtomicBool,
    }
    impl crate::broker_client::HostDialogs for SnapshotDialogs {
        fn pick_folder(&self, _s: Option<&str>) -> Result<Option<String>, String> {
            Ok(None)
        }
        fn verify_key(&self, _p: &str, _k: &str) -> Result<crate::broker_client::KeyVerdict, String> {
            Err("n/a".into())
        }
        fn fetch_catalog(&self, _pinned: &[String]) -> Result<crate::broker_client::CatalogFetch, String> {
            Err("n/a".into())
        }
        fn list_models(&self, reseller: &str, _t: Option<&str>) -> Result<Vec<crate::model_catalog::Model>, String> {
            self.live_called.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(vec![snap_model(reseller, &format!("live/{reseller}"))])
        }
        fn list_catalog_versions(&self) -> Result<Vec<crate::broker_client::CatalogVersion>, String> {
            Ok(vec![])
        }
        fn rollback_catalog(&self, _t: &str, _a: &[String]) -> Result<crate::broker_client::CatalogFetch, String> {
            Err("n/a".into())
        }
        fn fetch_model_snapshot(&self) -> Result<(String, String), String> {
            self.snapshot.lock().unwrap().clone().ok_or_else(|| "no snapshot".to_string())
        }
    }

    fn snap_model(reseller: &str, id: &str) -> crate::model_catalog::Model {
        crate::model_catalog::Model {
            id: id.into(),
            reseller: reseller.into(),
            display_name: id.into(),
            context_length: Some(128_000),
            pricing: crate::model_catalog::Pricing { prompt: Some(0.000003), completion: Some(0.000015) },
            modalities: vec!["text".into()],
            moderated: false,
        }
    }

    /// A deterministic ed25519 keypair (seed_hex, pubkey_hex) for signing test snapshots.
    fn snapshot_keypair(seed: u8) -> (String, String) {
        let k = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
        (hex::encode(k.to_bytes()), hex::encode(k.verifying_key().to_bytes()))
    }

    /// Build a signed snapshot's wire form `(index_json, sig_hex)` from a seed.
    fn signed_snapshot_wire(seed_hex: &str, version: u64, reseller: &str, id: &str) -> (String, String) {
        let snap = crate::model_snapshot::ModelSnapshot::new(
            version,
            "2026-07-15T00:00:00Z",
            vec![snap_model(reseller, id)],
        );
        let index = serde_json::to_string(&snap).unwrap();
        let sig = crate::model_snapshot::sign_snapshot(seed_hex, &snap).unwrap();
        (index, sig)
    }

    fn state_with_snapshot(dialogs: Arc<SnapshotDialogs>, keys: Vec<String>) -> ConsoleState {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("agents");
        let registry = AgentRegistry::load_from_dirs(&[dir]);
        ConsoleState::with_backends(
            registry,
            "/repo".into(),
            Arc::new(crate::broker_client::MemSecrets::default()),
            Arc::new(crate::presets::MemPresets::default()),
            dialogs,
            Arc::new(crate::history::MemHistory::default()),
        )
        .with_snapshot_keys(keys)
    }

    fn models_get(st: &ConsoleState, path: &str) -> Value {
        let r = route(st, &Request { method: "GET".into(), path: path.into(), ..Default::default() });
        assert_eq!(r.status, 200);
        serde_json::from_slice(&r.body).unwrap()
    }

    #[test]
    fn models_route_prefers_a_verified_snapshot_over_the_live_list() {
        let (seed, pk) = snapshot_keypair(11);
        let wire = signed_snapshot_wire(&seed, 5, "openrouter", "snap/model-a");
        let dialogs = Arc::new(SnapshotDialogs { snapshot: Mutex::new(Some(wire)), live_called: Default::default() });
        let st = state_with_snapshot(dialogs.clone(), vec![pk]);

        // Lazily downloaded + verified on the first request, then served (fast + offline).
        let v = models_get(&st, "/api/models?reseller=openrouter");
        assert_eq!(v["source"], "snapshot");
        assert_eq!(v["snapshotVersion"], 5);
        let models = v["models"].as_array().unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["id"], "snap/model-a");
        // A verified snapshot was present, so the live per-reseller fetch must NOT have run.
        assert!(!dialogs.live_called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn models_route_falls_back_to_live_when_the_snapshot_signature_is_untrusted() {
        let (seed, _pk) = snapshot_keypair(11);
        let (_seed2, other_pk) = snapshot_keypair(22); // trust a DIFFERENT key than the signer
        let wire = signed_snapshot_wire(&seed, 5, "openrouter", "snap/model-a");
        let dialogs = Arc::new(SnapshotDialogs { snapshot: Mutex::new(Some(wire)), live_called: Default::default() });
        let st = state_with_snapshot(dialogs.clone(), vec![other_pk]);

        let v = models_get(&st, "/api/models?reseller=openrouter");
        // Signature untrusted → snapshot ignored → the live list is served instead.
        assert_eq!(v["source"], "live");
        assert_eq!(v["models"][0]["id"], "live/openrouter");
        assert!(dialogs.live_called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn refresh_snapshot_rejects_a_tampered_snapshot() {
        let (seed, pk) = snapshot_keypair(11);
        let (index, sig) = signed_snapshot_wire(&seed, 5, "openrouter", "snap/model-a");
        // Mutate the index AFTER signing — the signature no longer covers it.
        let mut snap: crate::model_snapshot::ModelSnapshot = serde_json::from_str(&index).unwrap();
        snap.models[0].id = "snap/evil".into();
        let tampered = serde_json::to_string(&snap).unwrap();
        let dialogs = Arc::new(SnapshotDialogs { snapshot: Mutex::new(Some((tampered, sig))), live_called: Default::default() });
        let st = state_with_snapshot(dialogs, vec![pk]);
        assert!(!st.refresh_snapshot(), "a tampered snapshot must never be adopted");
    }

    #[test]
    fn refresh_snapshot_enforces_anti_rollback() {
        let (seed, pk) = snapshot_keypair(11);
        let dialogs = Arc::new(SnapshotDialogs {
            snapshot: Mutex::new(Some(signed_snapshot_wire(&seed, 5, "openrouter", "snap/new"))),
            live_called: Default::default(),
        });
        let st = state_with_snapshot(dialogs.clone(), vec![pk]);
        assert!(st.refresh_snapshot(), "v5 is the first snapshot → adopted");

        // Offer a strictly OLDER version — a rollback. It must be refused; the cache stays at v5.
        *dialogs.snapshot.lock().unwrap() = Some(signed_snapshot_wire(&seed, 3, "openrouter", "snap/old"));
        assert!(!st.refresh_snapshot(), "an older version is a rollback → refused");

        let v = models_get(&st, "/api/models?reseller=openrouter");
        assert_eq!(v["snapshotVersion"], 5);
        assert_eq!(v["models"][0]["id"], "snap/new");
    }

    #[test]
    fn models_route_refresh_1_forces_the_live_path() {
        let (seed, pk) = snapshot_keypair(11);
        let dialogs = Arc::new(SnapshotDialogs {
            snapshot: Mutex::new(Some(signed_snapshot_wire(&seed, 5, "openrouter", "snap/model-a"))),
            live_called: Default::default(),
        });
        let st = state_with_snapshot(dialogs.clone(), vec![pk]);
        assert!(st.refresh_snapshot()); // warm the cache

        let v = models_get(&st, "/api/models?reseller=openrouter&refresh=1");
        assert_eq!(v["source"], "live", "?refresh=1 forces the live per-reseller fetch");
        assert!(dialogs.live_called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn closing_a_session_announces_ended_for_leaf_sync() {
        use std::sync::{Arc, Mutex};
        let dir = tempfile::tempdir().unwrap();
        let agents = dir.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        let (cmd, args) = if cfg!(windows) {
            ("cmd", r#"["/c","ping","-n","30","127.0.0.1"]"#)
        } else {
            ("sh", r#"["-c","sleep 30"]"#)
        };
        let manifest = format!(
            r#"{{"schema_version":1,"id":"sleeper","name":"Sleeper",
               "run":{{"windows":{{"command":"{cmd}","args":{args}}},
                       "macos":{{"command":"{cmd}","args":{args}}},
                       "linux":{{"command":"{cmd}","args":{args}}}}},
               "providers":["native"],"provider_recipes":{{"native":{{"env":{{}},"args":[]}}}}}}"#
        );
        std::fs::write(agents.join("sleeper.json"), manifest).unwrap();
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let rec = Arc::clone(&seen);
        let st = ConsoleState::new(
            AgentRegistry::load_from_dirs(&[agents]),
            dir.path().to_string_lossy().into_owned(),
        )
        .with_announcer(Arc::new(move |p: String| rec.lock().unwrap().push(p)));

        let launch = route(&st, &Request {
            method: "POST".into(),
            path: "/api/launch".into(),
            body: r#"{"agent":"sleeper","provider":"native"}"#.into(),
            ..Default::default()
        });
        let id = serde_json::from_slice::<Value>(&launch.body).unwrap()["session"].as_str().unwrap().to_string();
        assert_eq!(st.close_all().len(), 1);
        // The close path announced an `ended` for that session id so the explorer removes its leaf.
        let announces = seen.lock().unwrap().clone();
        assert!(
            announces.iter().any(|a| a.contains("\"event\":\"ended\"") && a.contains(&id)),
            "expected an ended announce for {id}, got {announces:?}"
        );
    }

    #[test]
    fn reseller_keys_store_under_their_own_namespace_and_resolve_for_egress() {
        let dialogs = Arc::new(StubDialogs {
            verdict: crate::broker_client::KeyVerdict { valid: true, live: true, detail: String::new() },
            called: std::sync::atomic::AtomicBool::new(false),
        });
        let st = state_with_dialogs(dialogs);
        let post = |path: &str, body: &str| {
            route(&st, &Request { method: "POST".into(), path: path.into(), body: body.into(), ..Default::default() })
        };
        let get = |path: &str| {
            let r = route(&st, &Request { method: "GET".into(), path: path.into(), ..Default::default() });
            serde_json::from_slice::<Value>(&r.body).unwrap()
        };
        // Set a reseller key; it lands in the reseller: namespace (not provider:).
        assert_eq!(post("/api/reseller-keys", r#"{"reseller":"openrouter","key":"sk-or-abc123"}"#).status, 200);
        assert_eq!(st.secrets.get("reseller:openrouter").unwrap().as_deref(), Some("sk-or-abc123"));
        assert!(st.secrets.get("provider:openrouter").unwrap().is_none(), "must not collide with provider keys");
        // The list endpoint reports it (names only, never the value).
        let listed = get("/api/reseller-keys");
        assert!(listed["resellers"].as_array().unwrap().iter().any(|r| r["reseller"] == "openrouter"));
        // Missing fields are rejected.
        assert_eq!(post("/api/reseller-keys", r#"{"reseller":"","key":"x"}"#).status, 400);
        assert_eq!(post("/api/reseller-keys", r#"{"reseller":"groq","key":""}"#).status, 400);
        // Delete removes it.
        assert_eq!(post("/api/reseller-keys/delete", r#"{"reseller":"openrouter"}"#).status, 200);
        assert!(st.secrets.get("reseller:openrouter").unwrap().is_none());
        assert!(get("/api/reseller-keys")["resellers"].as_array().unwrap().is_empty());
    }

    #[test]
    fn key_verify_passes_through_a_live_provider_verdict() {
        // A definitive provider rejection comes back live:true, valid:false with the host's detail.
        let stub = Arc::new(StubDialogs {
            verdict: crate::broker_client::KeyVerdict {
                valid: false,
                live: true,
                detail: "Provider rejected this key (unauthorized).".into(),
            },
            called: Default::default(),
        });
        let st = state_with_dialogs(stub.clone());
        let r = route(&st, &Request {
            method: "POST".into(),
            path: "/api/keys/verify".into(),
            body: r#"{"provider":"openrouter","key":"sk-or-abcdef123456"}"#.into(),
            ..Default::default()
        });
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["live"], true);
        assert_eq!(v["valid"], false);
        assert_eq!(v["detail"], "Provider rejected this key (unauthorized).");
        assert!(stub.called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn catalog_versions_and_rollback_routes(){
        let stub = Arc::new(StubDialogs {
            verdict: crate::broker_client::KeyVerdict { valid: true, live: true, detail: String::new() },
            called: Default::default(),
        });
        let st = state_with_dialogs(stub);
        // Enumerate: the stub offers one version.
        let vs: Value = serde_json::from_slice(
            &route(&st, &Request { method: "GET".into(), path: "/api/catalog/versions".into(), ..Default::default() }).body,
        ).unwrap();
        assert_eq!(vs["versions"][0]["tag"], "v0.1.0");
        // Rollback: a tag + agents applies (stub returns applied:1).
        let post = |body: &str| route(&st, &Request { method: "POST".into(), path: "/api/catalog/rollback".into(), body: body.into(), ..Default::default() });
        let ok: Value = serde_json::from_slice(&post(r#"{"tag":"v0.1.0","agents":["claude"]}"#).body).unwrap();
        assert_eq!(ok["indexOk"], true);
        assert_eq!(ok["applied"], 1);
        assert_eq!(ok["tag"], "v0.1.0");
        // Guards: a missing tag or empty agents is a 400.
        assert_eq!(post(r#"{"agents":["claude"]}"#).status, 400);
        assert_eq!(post(r#"{"tag":"v0.1.0","agents":[]}"#).status, 400);
    }

    #[test]
    fn key_verify_skips_the_live_check_for_a_bad_format_key() {
        // A malformed key must be rejected offline — the host is never contacted.
        let stub = Arc::new(StubDialogs {
            verdict: crate::broker_client::KeyVerdict { valid: true, live: true, detail: String::new() },
            called: Default::default(),
        });
        let st = state_with_dialogs(stub.clone());
        let r = route(&st, &Request {
            method: "POST".into(),
            path: "/api/keys/verify".into(),
            body: r#"{"provider":"openrouter","key":"totally-wrong"}"#.into(),
            ..Default::default()
        });
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["valid"], false);
        assert_eq!(v["live"], false);
        assert!(!stub.called.load(std::sync::atomic::Ordering::SeqCst), "no network on a bad shape");
    }

    #[test]
    fn session_history_records_lists_and_serves_a_redacted_transcript() {
        let st = state(); // MemHistory backend
        let meta = SessionMeta {
            agent: "claude".into(),
            provider: "openrouter".into(),
            model: "sonnet".into(),
            cwd: "/repo".into(),
            started_at: "1720000000000".into(),
        };
        // Simulate a session ending with the injected key echoed into its transcript.
        record_session_end(&*st.history, &meta, "s1", "logged in with sk-or-topsecret\n".into(), &[
            "sk-or-topsecret".into(),
        ]);

        // GET /api/history lists it (metadata only, newest-first) and never the key.
        let list: Value = serde_json::from_slice(&get(&st, "/api/history").body).unwrap();
        assert_eq!(list["sessions"][0]["id"], "s1");
        assert_eq!(list["sessions"][0]["agent"], "claude");
        assert_eq!(list["sessions"][0]["model"], "sonnet");
        assert!(!String::from_utf8_lossy(&get(&st, "/api/history").body).contains("topsecret"));

        // GET /api/history/{id} returns the transcript with the secret redacted.
        let detail: Value = serde_json::from_slice(&get(&st, "/api/history/s1").body).unwrap();
        let transcript = detail["transcript"].as_str().unwrap();
        assert!(transcript.contains("***"), "secret should be redacted");
        assert!(!transcript.contains("topsecret"));

        // Unknown id → 400.
        assert_eq!(get(&st, "/api/history/nope").status, 400);
    }

    #[test]
    fn reload_catalog_hot_swaps_in_a_newly_signed_agent() {
        use ed25519_dalek::{Signer, SigningKey};
        let k = SigningKey::from_bytes(&[7u8; 32]);
        let pk = hex::encode(k.verifying_key().to_bytes());
        let signed = tempfile::tempdir().unwrap();
        let sources = CatalogSources {
            bundled: vec![],
            signed_dir: Some(signed.path().to_path_buf()),
            keys: vec![pk],
        };
        let st = ConsoleState::with_backends(
            sources.build(),
            "/repo".into(),
            Arc::new(crate::broker_client::MemSecrets::default()),
            Arc::new(crate::presets::MemPresets::default()),
            Arc::new(crate::broker_client::NoopDialogs),
            Arc::new(crate::history::MemHistory::default()),
        )
        .with_catalog_sources(sources);
        assert_eq!(st.registry.read().unwrap().len(), 0);

        // Drop a newly-signed manifest into the source, then reload via the endpoint.
        let m = br#"{"schema_version":1,"id":"newagent","name":"New","run":{"windows":{"command":"x"},"macos":{"command":"x"},"linux":{"command":"x"}}}"#;
        std::fs::write(signed.path().join("newagent.json"), m).unwrap();
        std::fs::write(signed.path().join("newagent.json.sig"), hex::encode(k.sign(m).to_bytes())).unwrap();

        let r: Value = serde_json::from_slice(
            &route(&st, &Request { method: "POST".into(), path: "/api/catalog/reload".into(), ..Default::default() }).body,
        )
        .unwrap();
        assert_eq!(r["agents"], 1);
        assert!(st.registry.read().unwrap().get("newagent").is_some());
    }

    #[test]
    fn catalog_reset_reverts_to_the_shipped_set() {
        use ed25519_dalek::{Signer, SigningKey};
        let k = SigningKey::from_bytes(&[8u8; 32]);
        let pk = hex::encode(k.verifying_key().to_bytes());
        let signed = tempfile::tempdir().unwrap();
        let sources = CatalogSources {
            bundled: vec![],
            signed_dir: Some(signed.path().to_path_buf()),
            keys: vec![pk],
        };
        let m = br#"{"schema_version":1,"id":"extra","name":"Extra","run":{"windows":{"command":"x"},"macos":{"command":"x"},"linux":{"command":"x"}}}"#;
        std::fs::write(signed.path().join("extra.json"), m).unwrap();
        std::fs::write(signed.path().join("extra.json.sig"), hex::encode(k.sign(m).to_bytes())).unwrap();
        std::fs::write(signed.path().join("versions.json"), "{}").unwrap();
        let st = ConsoleState::with_backends(
            sources.build(),
            "/repo".into(),
            Arc::new(crate::broker_client::MemSecrets::default()),
            Arc::new(crate::presets::MemPresets::default()),
            Arc::new(crate::broker_client::NoopDialogs),
            Arc::new(crate::history::MemHistory::default()),
        )
        .with_catalog_sources(sources);
        assert_eq!(st.registry.read().unwrap().len(), 1);

        let r: Value = serde_json::from_slice(
            &route(&st, &Request { method: "POST".into(), path: "/api/catalog/reset".into(), ..Default::default() }).body,
        )
        .unwrap();
        assert_eq!(r["agents"], 0);
        assert!(st.registry.read().unwrap().get("extra").is_none());
        assert!(!signed.path().join("extra.json").exists()); // fetched files cleared
        assert!(!signed.path().join("versions.json").exists());
    }


    #[test]
    fn catalog_auto_update_and_pin_persist(){
        let st = state();
        let post = |path: &str, body: &str| {
            route(&st, &Request { method: "POST".into(), path: path.into(), body: body.into(), ..Default::default() })
        };
        // Auto-update toggle persists and shows up in the catalog's presets.
        assert_eq!(post("/api/catalog/settings", r#"{"autoUpdate":true}"#).status, 200);
        let cat: Value = serde_json::from_slice(&get(&st, "/api/catalog").body).unwrap();
        assert_eq!(cat["presets"]["autoUpdateCatalog"], true);

        // Pin then unpin an agent.
        assert_eq!(post("/api/catalog/pin", r#"{"agent":"claude","pinned":true}"#).status, 200);
        let cat: Value = serde_json::from_slice(&get(&st, "/api/catalog").body).unwrap();
        assert_eq!(cat["presets"]["pinnedAgents"][0], "claude");
        post("/api/catalog/pin", r#"{"agent":"claude","pinned":false}"#);
        let cat: Value = serde_json::from_slice(&get(&st, "/api/catalog").body).unwrap();
        assert_eq!(cat["presets"]["pinnedAgents"].as_array().unwrap().len(), 0);

        // Bad input is rejected.
        assert_eq!(post("/api/catalog/pin", r#"{"pinned":true}"#).status, 400);
        assert_eq!(post("/api/catalog/settings", r#"{}"#).status, 400);
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

    #[test]
    fn multiple_labelled_credentials_per_provider() {
        let st = state();
        let post = |path: &str, body: &str| {
            route(&st, &Request { method: "POST".into(), path: path.into(), body: body.into(), ..Default::default() })
        };
        // Two labelled OpenRouter keys.
        assert_eq!(post("/api/keys", r#"{"provider":"openrouter","label":"work","key":"sk-or-workkey123"}"#).status, 200);
        assert_eq!(post("/api/keys", r#"{"provider":"openrouter","label":"personal","key":"sk-or-homekey123"}"#).status, 200);
        // Both listed by label — never a value.
        let body = String::from_utf8_lossy(&get(&st, "/api/keys").body).to_string();
        assert!(body.contains("work") && body.contains("personal"));
        assert!(!body.contains("sk-or-"), "credential list must not leak values");
        // Deleting one leaves the other.
        assert_eq!(post("/api/keys/delete", r#"{"provider":"openrouter","label":"work"}"#).status, 200);
        let body2 = String::from_utf8_lossy(&get(&st, "/api/keys").body).to_string();
        assert!(!body2.contains("work"));
        assert!(body2.contains("personal"));
    }

    #[test]
    fn onboarding_flag_starts_false_and_persists_true() {
        let st = state();
        let cat: Value = serde_json::from_slice(&get(&st, "/api/catalog").body).unwrap();
        assert_eq!(cat["presets"]["onboarded"], false);
        let r = route(&st, &Request { method: "POST".into(), path: "/api/onboarded".into(), ..Default::default() });
        assert_eq!(r.status, 200);
        let cat2: Value = serde_json::from_slice(&get(&st, "/api/catalog").body).unwrap();
        assert_eq!(cat2["presets"]["onboarded"], true);
    }

    #[test]
    fn close_reclaims_sessions_via_the_routes_one_and_all() {
        // A long-running agent so each launch leaves a live child to reclaim (CPE-442).
        let dir = tempfile::tempdir().unwrap();
        let agents = dir.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        let (cmd, args) = if cfg!(windows) {
            ("cmd", r#"["/c","ping","-n","30","127.0.0.1"]"#)
        } else {
            ("sh", r#"["-c","sleep 30"]"#)
        };
        let manifest = format!(
            r#"{{"schema_version":1,"id":"sleeper","name":"Sleeper",
               "run":{{"windows":{{"command":"{cmd}","args":{args}}},
                       "macos":{{"command":"{cmd}","args":{args}}},
                       "linux":{{"command":"{cmd}","args":{args}}}}},
               "providers":["native"],
               "provider_recipes":{{"native":{{"env":{{}},"args":[]}}}}}}"#
        );
        std::fs::write(agents.join("sleeper.json"), manifest).unwrap();
        let st = ConsoleState::new(
            AgentRegistry::load_from_dirs(&[agents]),
            dir.path().to_string_lossy().into_owned(),
        );

        let launch = |st: &ConsoleState| -> String {
            let r = route(
                st,
                &Request {
                    method: "POST".into(),
                    path: "/api/launch".into(),
                    body: r#"{"agent":"sleeper","provider":"native"}"#.into(),
                    ..Default::default()
                },
            );
            assert_eq!(r.status, 200, "launch failed: {}", String::from_utf8_lossy(&r.body));
            serde_json::from_slice::<Value>(&r.body).unwrap()["session"].as_str().unwrap().to_string()
        };
        let post = |st: &ConsoleState, path: &str| -> Value {
            let r = route(st, &Request { method: "POST".into(), path: path.into(), ..Default::default() });
            serde_json::from_slice::<Value>(&r.body).unwrap()
        };

        let id1 = launch(&st);
        let _id2 = launch(&st);
        assert_eq!(st.sessions.lock().unwrap().len(), 2, "two sessions should be live");

        // GET /api/sessions lists the running sessions so the launcher can reattach tabs after a
        // close/reopen (CPE-461); the name defaults to the agent when no tabName was sent.
        let listed: Value = serde_json::from_slice(
            &route(&st, &Request { method: "GET".into(), path: "/api/sessions".into(), ..Default::default() }).body,
        ).unwrap();
        let arr = listed["sessions"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert!(arr.iter().any(|s| s["id"] == id1.as_str() && s["name"] == "Sleeper"));
        // Each session carries a usage object, and the endpoint aggregates per agent/provider
        // (CPE-311). A freshly-launched `sleep` reports nothing, so figures are zero but present.
        let u = &arr[0]["usage"];
        assert!(u["inputTokens"].is_u64() && u["outputTokens"].is_u64() && u["costUsd"].is_number());
        assert!(listed["usageByAgent"].is_array(), "usage is aggregated per agent");
        assert!(listed["usageByProvider"].is_array(), "usage is aggregated per provider");

        // Close ONE via its route: the session is removed and its child reclaimed.
        assert_eq!(post(&st, &format!("/api/session/{id1}/close"))["closed"], true);
        assert_eq!(st.sessions.lock().unwrap().len(), 1, "one session should remain");
        // Closing an unknown id is a harmless no-op.
        assert_eq!(post(&st, "/api/session/nope/close")["closed"], false);

        // Close ALL reclaims the rest and empties the set — nothing left running.
        assert_eq!(post(&st, "/api/close-all")["closed"].as_array().unwrap().len(), 1);
        assert!(st.sessions.lock().unwrap().is_empty(), "sessions remained after close-all");
        // Idempotent with nothing open.
        assert!(post(&st, "/api/close-all")["closed"].as_array().unwrap().is_empty());
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
            reseller: None,
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

    /// Regression guard for the CPE-309 "no output in any tab" break: with the default (in-process)
    /// engine, a launched session's output must reach its replay ring via the `adopt_session`
    /// pipeline — i.e. the seam refactor didn't sever PTY → ring → WS.
    #[test]
    fn a_launched_session_streams_output_into_its_replay_ring() {
        use std::time::{Duration, Instant};
        let st = state(); // default LocalEngine
        let (program, args) = crate::pty::shell_command("echo ring-capture-works");
        let launch = crate::pty::PtyLaunch { program, args, cwd: None, env: BTreeMap::new(), rows: 24, cols: 80 };
        let io = st.engine.launch("t1", &launch).expect("local engine launches");
        st.adopt_session("t1".into(), io, "tab".into(), "agent".into(), "provider".into(), vec![], None);

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let seen = st
                .sessions
                .lock()
                .unwrap()
                .get("t1")
                .map(|s| String::from_utf8_lossy(&s.ring.lock().unwrap()).into_owned())
                .unwrap_or_default();
            if seen.contains("ring-capture-works") {
                break;
            }
            assert!(Instant::now() < deadline, "session output never reached the ring: {seen:?}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
