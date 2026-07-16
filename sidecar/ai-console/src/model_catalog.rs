//! Dynamic model catalog (CPE-444) — the provider-agnostic [`Model`] shape, a declarative
//! **reseller registry** ([`ResellerRegistry`], CPE-445), and the **OpenRouter** model-list
//! normalizer ([`parse_openrouter_models`], CPE-446).
//!
//! The AI Console should let a user pick *any* model a reseller offers, not the small static set
//! baked into agent manifests. That list changes constantly, so it is **data**: each reseller is a
//! declarative manifest (mirroring the agent registry `agents.rs` and the forge providers
//! `repos/providers.rs`), and the actual list is fetched + normalized to the common `Model` shape.
//! Everything here is **pure** (unit-tested, no network); the host performs the allow-listed fetch
//! (CPE-447) and a signed GitHub snapshot keeps it fresh + offline (CPE-450/451).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Per-token price in USD, as advertised by the reseller. Optional because not every source reports
/// it; treated as **advisory display data**, never trusted for billing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Pricing {
    pub prompt: Option<f64>,
    pub completion: Option<f64>,
}

/// One selectable model, normalized across resellers. This is what the picker renders and the launch
/// flow consumes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    /// The reseller's model id (what you pass at inference), e.g. `anthropic/claude-3.5-sonnet`.
    pub id: String,
    /// Which reseller offers it (manifest id), e.g. `openrouter`.
    pub reseller: String,
    pub display_name: String,
    pub context_length: Option<u64>,
    pub pricing: Pricing,
    /// Input modalities, e.g. `["text","image"]`.
    pub modalities: Vec<String>,
    /// Whether the reseller moderates/filters this model's output.
    pub moderated: bool,
}

/// The reseller-manifest schema version this build understands.
pub const RESELLER_SCHEMA_VERSION: u32 = 1;

/// Normalizers this build knows how to apply to a reseller's model-list response.
pub const KNOWN_NORMALIZERS: [&str; 3] = ["openrouter", "openai", "github"];

/// A declarative description of a model reseller — how to reach and drive its model-list endpoint.
/// Adding a reseller is dropping one of these (CPE-448), not host code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResellerManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    /// Full HTTPS URL of the model-list endpoint, e.g. `https://openrouter.ai/api/v1/models`.
    pub models_endpoint: String,
    /// How the stored key is presented: `bearer` | `github` | `none`.
    pub auth: String,
    /// API hosts this reseller uses — the egress allow-list contribution (CPE-447).
    pub api_hosts: Vec<String>,
    /// Which [`KNOWN_NORMALIZERS`] entry parses its response.
    pub normalizer: String,
    #[serde(default)]
    pub web_base: Option<String>,
    /// The API dialect this reseller's **inference** endpoint speaks — `anthropic` | `openai` —
    /// matched against an agent's `reseller_recipes` so it can be launched (CPE-468/471). `None` ⇒
    /// model-list only (not yet launch-capable).
    #[serde(default)]
    pub protocol: Option<String>,
    /// The inference base URL fed to a launch as `{base_url}` (e.g. `https://api.together.xyz/v1`).
    /// Distinct from `models_endpoint` (the list URL). `None` ⇒ not launch-capable (CPE-471).
    #[serde(default)]
    pub launch_base_url: Option<String>,
    #[serde(skip)]
    pub source_dir: Option<PathBuf>,
}

impl ResellerManifest {
    /// Reject a manifest we can't safely use, with a reason. Mirrors `ProviderManifest::validate`.
    fn validate(&self) -> Result<(), String> {
        if self.schema_version > RESELLER_SCHEMA_VERSION {
            return Err(format!("schema_version {} is newer than supported {RESELLER_SCHEMA_VERSION}", self.schema_version));
        }
        if self.id.trim().is_empty() {
            return Err("empty reseller id".into());
        }
        if self.name.trim().is_empty() {
            return Err(format!("reseller '{}' has an empty name", self.id));
        }
        if self.models_endpoint.trim().is_empty() || !self.models_endpoint.starts_with("https://") {
            return Err(format!("reseller '{}' needs an https models_endpoint", self.id));
        }
        if !matches!(self.auth.as_str(), "bearer" | "github" | "none") {
            return Err(format!("reseller '{}' has unknown auth '{}'", self.id, self.auth));
        }
        if !KNOWN_NORMALIZERS.contains(&self.normalizer.as_str()) {
            return Err(format!("reseller '{}' has unknown normalizer '{}'", self.id, self.normalizer));
        }
        // Launch fields (CPE-471) are optional, but if present must be well-formed: a known protocol
        // and an https base URL, so a malformed launch descriptor can't reach the launcher.
        if let Some(p) = &self.protocol {
            if !matches!(p.as_str(), "anthropic" | "openai") {
                return Err(format!("reseller '{}' has unknown protocol '{}'", self.id, p));
            }
        }
        if let Some(b) = &self.launch_base_url {
            if !b.starts_with("https://") {
                return Err(format!("reseller '{}' launch_base_url must be https", self.id));
            }
        }
        Ok(())
    }

    /// A launch [`ResellerDescriptor`](crate::routing::ResellerDescriptor) for this reseller, if it
    /// declares an inference protocol + base URL (CPE-468/471). `None` for a model-list-only reseller.
    pub fn descriptor(&self) -> Option<crate::routing::ResellerDescriptor> {
        let protocol = self.protocol.as_deref()?.trim();
        let base_url = self.launch_base_url.as_deref()?.trim();
        if protocol.is_empty() || base_url.is_empty() {
            return None;
        }
        Some(crate::routing::ResellerDescriptor {
            id: self.id.clone(),
            name: self.name.clone(),
            protocol: protocol.to_string(),
            base_url: base_url.to_string(),
        })
    }
}

/// A non-fatal problem loading a manifest — surfaced, never silently dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadWarning {
    pub path: PathBuf,
    pub reason: String,
}

/// The set of resellers this build can pull model lists from. Mirrors `ProviderRegistry`.
#[derive(Debug, Default)]
pub struct ResellerRegistry {
    resellers: Vec<ResellerManifest>,
    warnings: Vec<LoadWarning>,
}

impl ResellerRegistry {
    /// Load every `*.json` reseller manifest from `dirs` (later dirs override earlier ids). A bad
    /// manifest is recorded as a warning and skipped — one broken file never sinks the catalog.
    pub fn load_from_dirs(dirs: &[PathBuf]) -> ResellerRegistry {
        let mut reg = ResellerRegistry::default();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                reg.load_file(&path);
            }
        }
        reg
    }

    fn load_file(&mut self, path: &Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => return self.warn(path, format!("read failed: {e}")),
        };
        let mut manifest: ResellerManifest = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => return self.warn(path, format!("bad json: {e}")),
        };
        if let Err(reason) = manifest.validate() {
            return self.warn(path, reason);
        }
        manifest.source_dir = path.parent().map(Path::to_path_buf);
        // Later dirs override an earlier same-id reseller.
        if let Some(slot) = self.resellers.iter_mut().find(|m| m.id == manifest.id) {
            *slot = manifest;
        } else {
            self.resellers.push(manifest);
        }
    }

    fn warn(&mut self, path: &Path, reason: String) {
        self.warnings.push(LoadWarning { path: path.to_path_buf(), reason });
    }

    pub fn get(&self, id: &str) -> Option<&ResellerManifest> {
        self.resellers.iter().find(|m| m.id == id)
    }
    pub fn all(&self) -> impl Iterator<Item = &ResellerManifest> {
        self.resellers.iter()
    }
    pub fn len(&self) -> usize {
        self.resellers.len()
    }
    pub fn is_empty(&self) -> bool {
        self.resellers.is_empty()
    }
    pub fn warnings(&self) -> &[LoadWarning] {
        &self.warnings
    }

    /// Every **launch-capable** reseller as a descriptor (CPE-471) — what the launcher offers as
    /// providers and routes launches through (`compose_reseller_launch`). Model-list-only resellers
    /// (no `protocol`/`launch_base_url`) are excluded. Ordered by id for a stable menu.
    pub fn descriptors(&self) -> Vec<crate::routing::ResellerDescriptor> {
        let mut ds: Vec<_> = self.resellers.iter().filter_map(|m| m.descriptor()).collect();
        ds.sort_by(|a, b| a.id.cmp(&b.id));
        ds
    }

    /// The union of every reseller's `api_hosts` — the host's egress allow-list (CPE-447). Sorted +
    /// de-duplicated so it's stable.
    pub fn egress_allow_list(&self) -> Vec<String> {
        let mut hosts: Vec<String> =
            self.resellers.iter().flat_map(|m| m.api_hosts.iter().cloned()).collect();
        hosts.sort();
        hosts.dedup();
        hosts
    }
}

/// Parse OpenRouter's `GET /api/v1/models` response into normalized [`Model`]s (CPE-446). OpenRouter
/// returns `{ "data": [ { id, name, context_length, pricing:{prompt,completion}, architecture:{
/// input_modalities, … } , top_provider:{is_moderated} } … ] }`, with **prices as strings**. Total:
/// malformed/unexpected JSON yields an empty list rather than erroring. Sorted by id for stability.
pub fn parse_openrouter_models(json: &str) -> Vec<Model> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let items = match value.get("data").and_then(|d| d.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    let price = |p: &serde_json::Value, k: &str| -> Option<f64> {
        // OpenRouter reports per-token price as a decimal string; "0" means free.
        p.get(k).and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok())
    };
    let mut out: Vec<Model> = items
        .iter()
        .filter_map(|it| {
            let id = it.get("id")?.as_str()?.to_string();
            let display_name =
                it.get("name").and_then(|n| n.as_str()).unwrap_or(&id).to_string();
            let context_length = it.get("context_length").and_then(|c| c.as_u64());
            let pricing = it
                .get("pricing")
                .map(|p| Pricing { prompt: price(p, "prompt"), completion: price(p, "completion") })
                .unwrap_or_default();
            let modalities = it
                .get("architecture")
                .and_then(|a| a.get("input_modalities"))
                .and_then(|m| m.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_else(|| vec!["text".to_string()]);
            let moderated = it
                .get("top_provider")
                .and_then(|t| t.get("is_moderated"))
                .and_then(|m| m.as_bool())
                .unwrap_or(false);
            Some(Model { id, reseller: "openrouter".into(), display_name, context_length, pricing, modalities, moderated })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Normalize a reseller's model-list response into [`Model`]s using the normalizer implied by its
/// `id` (CPE-446/448). OpenAI-compatible resellers share the OpenRouter parser (a superset — extra
/// fields default gracefully); GitHub Models has its own `catalog/models` shape. Total on malformed.
pub fn normalize_models(reseller: &str, body: &str) -> Vec<Model> {
    let r = reseller.to_ascii_lowercase();
    if r == "github-models" || r.starts_with("github-models-") {
        parse_github_models(body)
    } else {
        // openrouter + every OpenAI-compatible reseller. Re-tag the reseller so rows show their source.
        let mut models = parse_openrouter_models(body);
        for m in &mut models {
            m.reseller = reseller.to_string();
        }
        models
    }
}

/// Parse GitHub Models' `GET /catalog/models` response — a JSON array of `{ id|name, publisher,
/// summary, supported_input_modalities, … }`. Total on malformed. Sorted by id.
pub fn parse_github_models(json: &str) -> Vec<Model> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let items = match value.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut out: Vec<Model> = items
        .iter()
        .filter_map(|it| {
            let id = it.get("id").or_else(|| it.get("name")).and_then(|v| v.as_str())?.to_string();
            let display_name = it.get("name").and_then(|n| n.as_str()).unwrap_or(&id).to_string();
            let modalities = it
                .get("supported_input_modalities")
                .and_then(|m| m.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_else(|| vec!["text".to_string()]);
            Some(Model {
                id,
                reseller: "github-models".into(),
                display_name,
                context_length: None,
                pricing: Pricing::default(),
                modalities,
                moderated: false,
            })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_manifest(dir: &Path, name: &str, json: &str) {
        let mut f = std::fs::File::create(dir.join(name)).unwrap();
        f.write_all(json.as_bytes()).unwrap();
    }

    #[test]
    fn registry_loads_valid_manifests_and_unions_egress_hosts() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            "openrouter.json",
            r#"{"schema_version":1,"id":"openrouter","name":"OpenRouter",
               "models_endpoint":"https://openrouter.ai/api/v1/models","auth":"bearer",
               "api_hosts":["openrouter.ai"],"normalizer":"openrouter"}"#,
        );
        write_manifest(
            dir.path(),
            "groq.json",
            r#"{"schema_version":1,"id":"groq","name":"Groq",
               "models_endpoint":"https://api.groq.com/openai/v1/models","auth":"bearer",
               "api_hosts":["api.groq.com"],"normalizer":"openai"}"#,
        );
        let reg = ResellerRegistry::load_from_dirs(&[dir.path().to_path_buf()]);
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.get("openrouter").unwrap().normalizer, "openrouter");
        assert_eq!(reg.egress_allow_list(), vec!["api.groq.com", "openrouter.ai"]);
        assert!(reg.warnings().is_empty());
    }

    #[test]
    fn launch_fields_derive_a_reseller_descriptor_only_when_present() {
        let dir = tempfile::tempdir().unwrap();
        // Launch-capable: has protocol + launch_base_url.
        write_manifest(
            dir.path(),
            "together.json",
            r#"{"schema_version":1,"id":"together","name":"Together AI",
               "models_endpoint":"https://api.together.xyz/v1/models","auth":"bearer",
               "api_hosts":["api.together.xyz"],"normalizer":"openai",
               "protocol":"openai","launch_base_url":"https://api.together.xyz/v1"}"#,
        );
        // Model-list only: no launch fields → no descriptor.
        write_manifest(
            dir.path(),
            "listonly.json",
            r#"{"schema_version":1,"id":"listonly","name":"ListOnly",
               "models_endpoint":"https://x.example/v1/models","auth":"bearer",
               "api_hosts":["x.example"],"normalizer":"openai"}"#,
        );
        let reg = ResellerRegistry::load_from_dirs(&[dir.path().to_path_buf()]);
        assert!(reg.warnings().is_empty());
        assert_eq!(reg.get("listonly").unwrap().descriptor(), None);
        let together = reg.get("together").unwrap().descriptor().expect("launch-capable");
        assert_eq!(together.protocol, "openai");
        assert_eq!(together.base_url, "https://api.together.xyz/v1");
        // descriptors() returns only the launch-capable one.
        let ds = reg.descriptors();
        assert_eq!(ds.len(), 1);
        assert_eq!(ds[0].id, "together");
    }

    #[test]
    fn malformed_launch_fields_are_rejected_as_warnings() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(
            dir.path(),
            "badproto.json",
            r#"{"schema_version":1,"id":"a","name":"A","models_endpoint":"https://a/v1/models",
               "auth":"bearer","api_hosts":["a"],"normalizer":"openai","protocol":"grpc"}"#,
        );
        write_manifest(
            dir.path(),
            "httpbase.json",
            r#"{"schema_version":1,"id":"b","name":"B","models_endpoint":"https://b/v1/models",
               "auth":"bearer","api_hosts":["b"],"normalizer":"openai",
               "protocol":"openai","launch_base_url":"http://b/v1"}"#,
        );
        let reg = ResellerRegistry::load_from_dirs(&[dir.path().to_path_buf()]);
        assert_eq!(reg.len(), 0, "both malformed manifests should be skipped");
        assert_eq!(reg.warnings().len(), 2);
    }

    #[test]
    fn the_bundled_resellers_expose_launch_descriptors() {
        // The migrated OpenAI-compatible resellers (CPE-471) must be launch-capable.
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resellers");
        let reg = ResellerRegistry::load_from_dirs(&[dir]);
        assert!(reg.warnings().is_empty(), "bundled resellers must all be valid: {:?}", reg.warnings());
        let ids: Vec<_> = reg.descriptors().into_iter().map(|d| d.id).collect();
        for expected in [
            "together", "groq", "fireworks", "deepinfra", "novita", "aimlapi",
            "cerebras", "sambanova", "nebius", "hyperbolic",
            "mistral", "deepseek", "cohere",
        ] {
            assert!(ids.contains(&expected.to_string()), "{expected} should be launch-capable; got {ids:?}");
        }
        let groq = reg.get("groq").unwrap().descriptor().unwrap();
        assert_eq!(groq.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(groq.protocol, "openai");
    }

    #[test]
    fn registry_rejects_bad_manifests_with_warnings_not_panics() {
        let dir = tempfile::tempdir().unwrap();
        write_manifest(dir.path(), "no-id.json", r#"{"schema_version":1,"id":"","name":"X","models_endpoint":"https://x/y","auth":"bearer","api_hosts":[],"normalizer":"openai"}"#);
        write_manifest(dir.path(), "http.json", r#"{"schema_version":1,"id":"x","name":"X","models_endpoint":"http://x/y","auth":"bearer","api_hosts":[],"normalizer":"openai"}"#);
        write_manifest(dir.path(), "badauth.json", r#"{"schema_version":1,"id":"y","name":"Y","models_endpoint":"https://y/z","auth":"password","api_hosts":[],"normalizer":"openai"}"#);
        write_manifest(dir.path(), "future.json", r#"{"schema_version":999,"id":"z","name":"Z","models_endpoint":"https://z/z","auth":"none","api_hosts":[],"normalizer":"openai"}"#);
        let reg = ResellerRegistry::load_from_dirs(&[dir.path().to_path_buf()]);
        assert!(reg.is_empty(), "no bad manifest should load");
        assert_eq!(reg.warnings().len(), 4);
    }

    #[test]
    fn parses_openrouter_models_with_string_prices_and_modalities() {
        let json = r#"{"data":[
            {"id":"anthropic/claude-3.5-sonnet","name":"Claude 3.5 Sonnet","context_length":200000,
             "pricing":{"prompt":"0.000003","completion":"0.000015"},
             "architecture":{"input_modalities":["text","image"]},"top_provider":{"is_moderated":true}},
            {"id":"meta-llama/llama-3-8b","name":"Llama 3 8B","context_length":8192,
             "pricing":{"prompt":"0","completion":"0"}}
        ]}"#;
        let models = parse_openrouter_models(json);
        assert_eq!(models.len(), 2);
        // Sorted by id: "anthropic/..." before "meta-llama/...".
        let c = &models[0];
        assert_eq!(c.id, "anthropic/claude-3.5-sonnet");
        assert_eq!(c.reseller, "openrouter");
        assert_eq!(c.context_length, Some(200000));
        assert_eq!(c.pricing.prompt, Some(0.000003));
        assert_eq!(c.modalities, vec!["text", "image"]);
        assert!(c.moderated);
        // Missing architecture defaults modality to ["text"]; missing moderation → false.
        let l = &models[1];
        assert_eq!(l.modalities, vec!["text"]);
        assert!(!l.moderated);
        assert_eq!(l.pricing.completion, Some(0.0));
    }

    #[test]
    fn normalize_dispatches_by_reseller_and_retags_source() {
        // OpenAI-compatible reseller reuses the OpenRouter parser but is tagged with its own id.
        let openai_shape = r#"{"data":[{"id":"llama-3.1-70b","name":"Llama 3.1 70B"}]}"#;
        let m = normalize_models("groq", openai_shape);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].reseller, "groq");
        // GitHub Models uses its own array shape.
        let gh = r#"[{"id":"openai/gpt-4o","name":"GPT-4o","supported_input_modalities":["text","image"]}]"#;
        let g = normalize_models("github-models", gh);
        assert_eq!(g.len(), 1);
        assert_eq!((g[0].reseller.as_str(), g[0].id.as_str()), ("github-models", "openai/gpt-4o"));
        assert_eq!(g[0].modalities, vec!["text", "image"]);
        // Malformed → empty, never a panic.
        assert!(normalize_models("github-models", "nope").is_empty());
    }

    #[test]
    fn malformed_openrouter_json_yields_no_models() {
        assert!(parse_openrouter_models("not json").is_empty());
        assert!(parse_openrouter_models("{}").is_empty()); // no data array
        assert!(parse_openrouter_models(r#"{"data":[]}"#).is_empty());
        // An entry with no id is skipped, not fatal.
        assert!(parse_openrouter_models(r#"{"data":[{"name":"x"}]}"#).is_empty());
    }
}
