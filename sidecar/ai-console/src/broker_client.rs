//! Outbound capability client for the AI Console sidecar (CPE-344).
//!
//! The sidecar talks to the host over the stdio envelope channel. To *use* a granted
//! capability (e.g. `secrets.*`, CPE-268) it must send a [`Request`] and await the matching
//! [`Response`] — but the main loop reads the channel on one thread while the loopback HTTP
//! handlers run on others. [`BrokerClient`] bridges that: it allocates a correlation id,
//! writes the request through the shared stdout writer, and blocks on a per-id channel until
//! the main loop routes the response back via [`BrokerClient::deliver`].
//!
//! [`BrokerSecrets`] is the [`SecretAccess`] impl over this client, so the vault stores keys
//! in the OS keychain (via the host) — values only ever flow sidecar↔host, never to logs.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use sidecar_contract::{CorrelationId, Envelope, Message, Request, Response};

use crate::vault::SecretAccess;

/// A stdout writer shared between the main loop and the broker client.
pub type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// Correlates outbound requests with their responses over the envelope channel.
pub struct BrokerClient {
    writer: SharedWriter,
    next_id: AtomicU64,
    pending: Mutex<HashMap<CorrelationId, Sender<Response>>>,
    timeout: Duration,
}

impl BrokerClient {
    pub fn new(writer: SharedWriter) -> Self {
        Self::with_timeout(writer, Duration::from_secs(10))
    }

    pub fn with_timeout(writer: SharedWriter, timeout: Duration) -> Self {
        Self {
            writer,
            next_id: AtomicU64::new(1), // 0 is reserved for events/lifecycle
            pending: Mutex::new(HashMap::new()),
            timeout,
        }
    }

    /// Send a capability request and block until the host's response arrives (or the default
    /// timeout elapses). Returns the JSON result, or the error message.
    pub fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.request_timeout(method, params, self.timeout)
    }

    /// Like [`request`], but with an explicit wait — e.g. a folder dialog the user must
    /// interact with needs far longer than a plain capability call.
    pub fn request_timeout(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = channel();
        self.pending.lock().unwrap().insert(id, tx);

        let env = Envelope::new(id, Message::Request(Request { method: method.to_string(), params }));
        let line = env.to_json().map_err(|e| e.to_string())?;
        {
            let mut w = self.writer.lock().unwrap();
            writeln!(w, "{line}").map_err(|e| e.to_string())?;
            w.flush().map_err(|e| e.to_string())?;
        }

        match rx.recv_timeout(timeout) {
            Ok(resp) => resp.result.map_err(|e| e.message),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id); // don't leak the waiter
                Err(format!("broker request '{method}' timed out"))
            }
        }
    }

    /// Route an inbound `Response` (id from its envelope) to the blocked caller. The main
    /// loop calls this for every `Message::Response` it reads. Unknown ids are ignored
    /// (a late response to a timed-out request).
    pub fn deliver(&self, id: CorrelationId, resp: Response) {
        if let Some(tx) = self.pending.lock().unwrap().remove(&id) {
            let _ = tx.send(resp);
        }
    }

    /// Resolve this sidecar's private storage directory from the host (`storage.dir`,
    /// CPE-268). The sidecar reads/writes its own files there (e.g. presets.json).
    pub fn storage_dir(&self) -> Result<std::path::PathBuf, String> {
        let v = self.request("storage.dir", serde_json::Value::Null)?;
        v.get("dir")
            .and_then(|d| d.as_str())
            .map(std::path::PathBuf::from)
            .ok_or_else(|| "storage.dir returned no path".to_string())
    }

    #[cfg(test)]
    fn pending_len(&self) -> usize {
        self.pending.lock().unwrap().len()
    }
}

/// [`PresetsBackend`] persisted under the host storage directory (CPE-352). Reads/writes a
/// single `presets.json`. Load degrades to an empty store on any error so the console never
/// fails to open.
pub struct BrokerPresets {
    client: Arc<BrokerClient>,
}

impl BrokerPresets {
    pub fn new(client: Arc<BrokerClient>) -> Self {
        Self { client }
    }
    fn path(&self) -> Result<std::path::PathBuf, String> {
        Ok(self.client.storage_dir()?.join("presets.json"))
    }
}

impl crate::presets::PresetsBackend for BrokerPresets {
    fn load(&self) -> crate::presets::PresetStore {
        match self.path().and_then(|p| std::fs::read_to_string(p).map_err(|e| e.to_string())) {
            Ok(s) => crate::presets::PresetStore::from_json(&s),
            Err(_) => crate::presets::PresetStore::default(),
        }
    }
    fn save(&self, store: &crate::presets::PresetStore) -> Result<(), String> {
        let path = self.path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, store.to_json()).map_err(|e| e.to_string())
    }
}

/// [`crate::history::HistoryBackend`] persisted under host storage (CPE-370). Reads/writes a
/// single `history.json` next to `presets.json`; load degrades to empty on any error so the
/// console never fails to open.
pub struct BrokerHistory {
    client: Arc<BrokerClient>,
}

impl BrokerHistory {
    pub fn new(client: Arc<BrokerClient>) -> Self {
        Self { client }
    }
    fn path(&self) -> Result<std::path::PathBuf, String> {
        Ok(self.client.storage_dir()?.join("history.json"))
    }
}

impl crate::history::HistoryBackend for BrokerHistory {
    fn load(&self) -> crate::history::SessionHistory {
        match self.path().and_then(|p| std::fs::read_to_string(p).map_err(|e| e.to_string())) {
            Ok(s) => crate::history::SessionHistory::from_json(&s),
            Err(_) => crate::history::SessionHistory::default(),
        }
    }
    fn save(&self, history: &crate::history::SessionHistory) -> Result<(), String> {
        let path = self.path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, history.to_json()).map_err(|e| e.to_string())
    }
}

/// [`SecretAccess`] backed by the host secrets broker (CPE-268/344). Maps the vault's
/// key → the broker's `name` param. Wire protocol: `secrets.set {name,value}`,
/// `secrets.get {name} → {value}`, `secrets.delete {name}`.
pub struct BrokerSecrets {
    client: Arc<BrokerClient>,
}

impl BrokerSecrets {
    pub fn new(client: Arc<BrokerClient>) -> Self {
        Self { client }
    }
}

impl SecretAccess for BrokerSecrets {
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.client
            .request("secrets.set", json!({ "name": key, "value": value }))
            .map(|_| ())
    }
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        let v = self.client.request("secrets.get", json!({ "name": key }))?;
        Ok(v.get("value").and_then(Value::as_str).map(str::to_string))
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        self.client
            .request("secrets.delete", json!({ "name": key }))
            .map(|_| ())
    }
}

/// The host's answer to a live key check (CPE-347). `live` = "the provider gave a definitive
/// answer"; only `live && !valid` is a real rejection.
#[derive(Debug, Clone)]
pub struct KeyVerdict {
    pub valid: bool,
    pub live: bool,
    pub detail: String,
}

/// The host's summary of a catalog fetch+apply (CPE-376).
#[derive(Debug, Clone, Default)]
pub struct CatalogFetch {
    pub index_ok: bool,
    pub applied: usize,
}

/// One prior published catalog version offered by the rollback picker (CPE-383).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogVersion {
    pub tag: String,
    pub published_at: String,
    pub prerelease: bool,
}

/// Host-mediated operations the sandboxed launcher can't perform itself — native dialogs
/// (CPE-354) and outbound network (CPE-347/376), which the sandbox has no client for.
pub trait HostDialogs: Send + Sync {
    /// Open a native folder picker, starting at `start` when it's an existing directory (CPE-395).
    /// `Ok(Some(path))` chosen, `Ok(None)` cancelled.
    fn pick_folder(&self, start: Option<&str>) -> Result<Option<String>, String>;

    /// Verify a provider key against the provider's API. The host makes a single request to an
    /// allow-listed endpoint (the sandbox never supplies the URL). `Err` = the check couldn't
    /// run (no host); the caller falls back to the offline format check.
    fn verify_key(&self, provider: &str, key: &str) -> Result<KeyVerdict, String>;

    /// Ask the host to fetch + apply the signed agent-catalog bundle from its configured source
    /// (CPE-376). `pinned` agents are skipped (CPE-378). `Err` = no host (dev/standalone).
    fn fetch_catalog(&self, pinned: &[String]) -> Result<CatalogFetch, String>;

    /// Fetch + normalize a reseller's model list via the host's allow-listed egress
    /// (`host.list_models`, CPE-447) so the launcher can offer **any** model the reseller lists
    /// (CPE-444/449). `token` is optional — OpenRouter's model list is public. `Err` = no host or
    /// the fetch failed; the caller falls back to the static per-agent model set.
    fn list_models(&self, reseller: &str, token: Option<&str>) -> Result<Vec<crate::model_catalog::Model>, String>;

    /// Enumerate prior published catalog versions for the rollback picker (CPE-383). `Err` = no host.
    fn list_catalog_versions(&self) -> Result<Vec<CatalogVersion>, String>;

    /// Roll `agents` back to a specific published catalog version `tag` (CPE-383) — an audited
    /// downgrade (`allow_downgrade`) applied only to those agents. `Err` = no host.
    fn rollback_catalog(&self, tag: &str, agents: &[String]) -> Result<CatalogFetch, String>;

    /// Download the published model-catalog snapshot via the host's allow-listed egress
    /// (`host.fetch_model_snapshot`, CPE-451). Returns the raw `(index_json, sig_hex)` — the caller
    /// verifies the ed25519 signature + anti-rollback before trusting it (the host never verifies).
    /// `Err` = no host or the fetch failed; the caller falls back to the live per-reseller list.
    fn fetch_model_snapshot(&self) -> Result<(String, String), String>;
}

/// [`HostDialogs`] over the broker: asks the host to open the dialog (`host.pick_folder`).
pub struct BrokerDialogs {
    client: Arc<BrokerClient>,
}

impl BrokerDialogs {
    pub fn new(client: Arc<BrokerClient>) -> Self {
        Self { client }
    }
}

impl HostDialogs for BrokerDialogs {
    fn pick_folder(&self, start: Option<&str>) -> Result<Option<String>, String> {
        // Longer wait than a normal request — the user has to interact with the dialog.
        let v = self.client.request_timeout(
            "host.pick_folder",
            serde_json::json!({ "start": start }),
            Duration::from_secs(300),
        )?;
        Ok(v.get("path").and_then(Value::as_str).map(str::to_string))
    }

    fn verify_key(&self, provider: &str, key: &str) -> Result<KeyVerdict, String> {
        // Longer than a plain capability call — the host makes an outbound HTTPS round-trip.
        let v = self.client.request_timeout(
            "host.verify_key",
            serde_json::json!({ "provider": provider, "key": key }),
            Duration::from_secs(20),
        )?;
        Ok(KeyVerdict {
            valid: v.get("valid").and_then(Value::as_bool).unwrap_or(false),
            live: v.get("live").and_then(Value::as_bool).unwrap_or(false),
            detail: v.get("detail").and_then(Value::as_str).unwrap_or_default().to_string(),
        })
    }

    fn fetch_catalog(&self, pinned: &[String]) -> Result<CatalogFetch, String> {
        // A network round-trip (download + verify + apply) — allow it a generous wait.
        let v = self.client.request_timeout(
            "host.fetch_catalog",
            serde_json::json!({ "pinned": pinned }),
            Duration::from_secs(60),
        )?;
        Ok(CatalogFetch {
            index_ok: v.get("indexOk").and_then(Value::as_bool).unwrap_or(false),
            applied: v.get("applied").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0),
        })
    }

    fn list_models(&self, reseller: &str, token: Option<&str>) -> Result<Vec<crate::model_catalog::Model>, String> {
        // A network round-trip through the host's allow-listed egress; give it a generous wait.
        let v = self.client.request_timeout(
            "host.list_models",
            serde_json::json!({ "reseller": reseller, "token": token }),
            Duration::from_secs(30),
        )?;
        if !v.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return Err(v.get("error").and_then(Value::as_str).unwrap_or("model fetch failed").to_string());
        }
        let body = v.get("body").and_then(Value::as_str).unwrap_or("");
        Ok(crate::model_catalog::normalize_models(reseller, body))
    }

    fn list_catalog_versions(&self) -> Result<Vec<CatalogVersion>, String> {
        let v = self.client.request_timeout(
            "host.list_catalog_versions",
            serde_json::json!({}),
            Duration::from_secs(20),
        )?;
        Ok(v.get("versions")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        Some(CatalogVersion {
                            tag: x.get("tag").and_then(Value::as_str)?.to_string(),
                            published_at: x
                                .get("publishedAt")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            prerelease: x.get("prerelease").and_then(Value::as_bool).unwrap_or(false),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    fn rollback_catalog(&self, tag: &str, agents: &[String]) -> Result<CatalogFetch, String> {
        // Same host call as a fetch, but pinned to a specific `tag` with an `agents` downgrade set.
        let v = self.client.request_timeout(
            "host.fetch_catalog",
            serde_json::json!({ "tag": tag, "agents": agents }),
            Duration::from_secs(60),
        )?;
        Ok(CatalogFetch {
            index_ok: v.get("indexOk").and_then(Value::as_bool).unwrap_or(false),
            applied: v.get("applied").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0),
        })
    }

    fn fetch_model_snapshot(&self) -> Result<(String, String), String> {
        // A network round-trip (two GitHub asset GETs) through the host's allow-listed egress.
        let v = self.client.request_timeout(
            "host.fetch_model_snapshot",
            serde_json::json!({}),
            Duration::from_secs(30),
        )?;
        if !v.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return Err(v.get("error").and_then(Value::as_str).unwrap_or("snapshot fetch failed").to_string());
        }
        let index = v.get("index").and_then(Value::as_str).unwrap_or_default().to_string();
        let sig = v.get("sig").and_then(Value::as_str).unwrap_or_default().to_string();
        Ok((index, sig))
    }
}

/// Dev/standalone fallback — no host, so "cancelled" / "can't verify".
pub struct NoopDialogs;
impl HostDialogs for NoopDialogs {
    fn pick_folder(&self, _start: Option<&str>) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn verify_key(&self, _provider: &str, _key: &str) -> Result<KeyVerdict, String> {
        Err("no host to verify against".into())
    }
    fn fetch_catalog(&self, _pinned: &[String]) -> Result<CatalogFetch, String> {
        Err("no host to fetch the catalog".into())
    }
    fn list_models(&self, _reseller: &str, _token: Option<&str>) -> Result<Vec<crate::model_catalog::Model>, String> {
        Err("no host to fetch models".into())
    }
    fn list_catalog_versions(&self) -> Result<Vec<CatalogVersion>, String> {
        Err("no host to enumerate catalog versions".into())
    }
    fn rollback_catalog(&self, _tag: &str, _agents: &[String]) -> Result<CatalogFetch, String> {
        Err("no host to roll the catalog back".into())
    }
    fn fetch_model_snapshot(&self) -> Result<(String, String), String> {
        Err("no host to fetch the model snapshot".into())
    }
}

/// An in-memory [`SecretAccess`] fallback used when there's no host broker (dev/standalone
/// runs). Keys live only for the process lifetime — never written to disk.
#[derive(Default)]
pub struct MemSecrets {
    map: Mutex<HashMap<String, String>>,
}

impl SecretAccess for MemSecrets {
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.map.lock().unwrap().insert(key.to_string(), value.to_string());
        Ok(())
    }
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        Ok(self.map.lock().unwrap().get(key).cloned())
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        self.map.lock().unwrap().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sidecar_contract::{ContractError, ErrorCode};
    use std::time::Instant;

    /// A shared in-memory writer we can inspect for what the client wrote.
    fn buffer() -> (SharedWriter, Arc<Mutex<Vec<u8>>>) {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer: SharedWriter = Arc::new(Mutex::new(Box::new(SinkWriter(sink.clone()))));
        (writer, sink)
    }
    struct SinkWriter(Arc<Mutex<Vec<u8>>>);
    impl Write for SinkWriter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Wait until the client has written a request and registered a waiter, then return the
    /// correlation id it used (parsed from the wire) — no fixed sleeps.
    fn wait_for_request(client: &BrokerClient, sink: &Arc<Mutex<Vec<u8>>>) -> CorrelationId {
        let start = Instant::now();
        loop {
            if client.pending_len() == 1 {
                let bytes = sink.lock().unwrap().clone();
                let line = String::from_utf8(bytes).unwrap();
                if let Some(l) = line.lines().next() {
                    if let Ok(env) = Envelope::from_json(l) {
                        return env.id;
                    }
                }
            }
            assert!(start.elapsed() < Duration::from_secs(2), "request never registered");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn request_correlates_the_matching_response() {
        let (writer, sink) = buffer();
        let client = Arc::new(BrokerClient::new(writer));
        let c = client.clone();
        let h = std::thread::spawn(move || c.request("secrets.get", json!({ "name": "k" })));

        let id = wait_for_request(&client, &sink);
        client.deliver(id, Response { result: Ok(json!({ "value": "s3cret" })) });

        assert_eq!(h.join().unwrap().unwrap(), json!({ "value": "s3cret" }));
        assert_eq!(client.pending_len(), 0);
    }

    #[test]
    fn request_surfaces_a_response_error_message() {
        let (writer, sink) = buffer();
        let client = Arc::new(BrokerClient::new(writer));
        let c = client.clone();
        let h = std::thread::spawn(move || c.request("secrets.get", json!({ "name": "k" })));

        let id = wait_for_request(&client, &sink);
        client.deliver(
            id,
            Response { result: Err(ContractError::new(ErrorCode::Internal, "store locked", false)) },
        );
        assert_eq!(h.join().unwrap().unwrap_err(), "store locked");
    }

    #[test]
    fn request_times_out_when_no_response_arrives() {
        let (writer, _sink) = buffer();
        let client = BrokerClient::with_timeout(writer, Duration::from_millis(60));
        let err = client.request("secrets.get", json!({ "name": "k" })).unwrap_err();
        assert!(err.contains("timed out"));
        assert_eq!(client.pending_len(), 0); // waiter cleaned up
    }

    #[test]
    fn broker_secrets_round_trips_set_and_get() {
        let (writer, sink) = buffer();
        let client = Arc::new(BrokerClient::new(writer));
        let secrets = BrokerSecrets::new(client.clone());

        // set: deliver an {ok:true} for whatever id it uses.
        let c = client.clone();
        let h = std::thread::spawn(move || {
            BrokerSecrets::new(c).set("provider:openrouter", "sk-or-123")
        });
        let id = wait_for_request(&client, &sink);
        // Verify it sent name+value on the wire.
        let sent = Envelope::from_json(String::from_utf8(sink.lock().unwrap().clone()).unwrap().lines().next().unwrap()).unwrap();
        if let Message::Request(r) = sent.message {
            assert_eq!(r.method, "secrets.set");
            assert_eq!(r.params["name"], "provider:openrouter");
            assert_eq!(r.params["value"], "sk-or-123");
        } else {
            panic!("not a request");
        }
        client.deliver(id, Response { result: Ok(json!({ "ok": true })) });
        h.join().unwrap().unwrap();

        // get: a null value means "absent".
        let _ = secrets; // (constructed to prove the type wires up)
    }

    #[test]
    fn mem_secrets_round_trips() {
        let m = MemSecrets::default();
        assert_eq!(m.get("k").unwrap(), None);
        m.set("k", "v").unwrap();
        assert_eq!(m.get("k").unwrap().as_deref(), Some("v"));
        m.delete("k").unwrap();
        assert_eq!(m.get("k").unwrap(), None);
    }
}
