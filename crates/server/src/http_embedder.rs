//! Real, configurable [`Embedder`](crate::embedder::Embedder) over an **OpenAI-compatible**
//! `/embeddings` endpoint (CPE-1273, epic CPE-976). It lets a user plug a genuine embedding model into
//! content search (CPE-1262/1263) — a LOCAL server (LM Studio / Ollama, no key) or a hosted one
//! (OpenAI/others, with a key) — WITHOUT the repo ever shipping a key or a bundled model.
//!
//! # Why this shape
//! OpenAI's `/v1/embeddings` request/response (`{ "model", "input": [...] }` →
//! `{ "data": [ { "embedding": [f32, …] } ] }`) is the de-facto standard every local + hosted server
//! speaks, and it fits the user's installed LM Studio. HTTP is over **`ureq`** — already a workspace
//! dependency ([`cpe-webdav`] + the app adapter), so no new HTTP stack enters the tree — and only under
//! the `http-embedder` feature, so the lean default `cpe-server` build pulls in zero HTTP/TLS code.
//!
//! # Testable without a network
//! The transport is a seam ([`EmbeddingsTransport`]): the pure request-building
//! ([`build_request_body`]), response-parsing ([`parse_embeddings_response`]), and URL-normalisation
//! ([`embeddings_url`]) are plain functions with unit tests, and [`HttpEmbedder::with_transport`] drives
//! the whole embed flow over an **injected** transport, so the request→parse→dim path is exercised
//! headlessly with a fake — **no real network in any test**. The real `ureq` transport
//! ([`HttpEmbedder::connect`]) is feature-gated; end-to-end quality is the user's own endpoint.
//!
//! # Never panics, never leaks the key
//! Every failure (unreachable endpoint, bad key, malformed body) maps to a clear `Err`; a mid-build
//! embed failure degrades to zero vectors (that chunk simply won't match) rather than aborting the whole
//! build. The API key travels only in the `Authorization` header and is **never** placed in an error
//! message, a log line, or any returned value.

use serde::{Deserialize, Serialize};

use crate::embedder::Embedder;

/// A fixed, tiny probe string embedded once at connect time to discover the model's vector
/// dimensionality (the [`Embedder::dim`] the [`crate::vector_index`] must be sized to) and to prove the
/// endpoint + key work before a real build/search runs.
const PROBE_TEXT: &str = "cpe content-search embedding probe";

/// The transport seam: POST a JSON `body` to `url` with an optional bearer token, returning the raw
/// response bytes or a legible error. Abstracted off [`HttpEmbedder`] so the request/parse flow is
/// unit-testable with a fake (no real network); the production impl over `ureq` is
/// [`connect`](HttpEmbedder::connect)-only and feature-gated.
///
/// Contract: the `bearer` token MUST NOT appear in the returned error string (it is a secret).
pub trait EmbeddingsTransport: Send + Sync {
    fn post(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<Vec<u8>, String>;
}

/// The `/embeddings` request body (`{ "model": …, "input": [texts…] }`) — the OpenAI-compatible shape
/// every target server accepts. Serialised by [`build_request_body`].
#[derive(Debug, Serialize)]
struct EmbeddingsRequest<'a> {
    model: &'a str,
    input: &'a [&'a str],
}

/// One item of an OpenAI-format embeddings response's `data` array. `index` echoes the position of the
/// corresponding input; we sort by it so a server that reorders `data` still aligns with the inputs.
#[derive(Debug, Deserialize)]
struct EmbeddingDatum {
    #[serde(default)]
    index: usize,
    embedding: Vec<f32>,
}

/// The `/embeddings` response envelope we read: just its `data` array. Extra fields (`model`, `usage`,
/// `object`, …) are ignored, so any OpenAI-compatible server's response parses.
#[derive(Debug, Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingDatum>,
}

/// Normalise a user-supplied base URL into the full `/embeddings` endpoint, accepting it **with or
/// without** the `/v1` version segment (the ticket's requirement):
///
/// - `http://host:1234/v1/embeddings` → used as-is (already a full endpoint).
/// - `http://host:1234/v1`            → `…/v1/embeddings` (LM Studio's own base URL shape).
/// - `http://host:1234`               → `…/v1/embeddings` (bare host — default to the versioned path).
/// - `https://api.openai.com/v1`      → `…/v1/embeddings`.
///
/// Trailing slashes are trimmed first so `…/v1/` behaves like `…/v1`.
pub fn embeddings_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/embeddings") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/embeddings")
    } else {
        format!("{trimmed}/v1/embeddings")
    }
}

/// Serialise the request body for `model` + `texts` — pure, so it's unit-tested directly.
pub fn build_request_body(model: &str, texts: &[&str]) -> String {
    // Serialising a plain struct of borrowed data can't fail; fall back to a hand-built body defensively
    // rather than unwrap (never panic).
    serde_json::to_string(&EmbeddingsRequest { model, input: texts }).unwrap_or_else(|_| {
        format!(r#"{{"model":{},"input":[]}}"#, serde_json::to_string(model).unwrap_or_else(|_| "\"\"".into()))
    })
}

/// Parse an OpenAI-format embeddings response body into the per-input vectors, ordered to match the
/// inputs (by each datum's `index`). A malformed/empty body is a clear `Err`, never a panic.
pub fn parse_embeddings_response(bytes: &[u8]) -> Result<Vec<Vec<f32>>, String> {
    let mut resp: EmbeddingsResponse = serde_json::from_slice(bytes)
        .map_err(|e| format!("embeddings response was not valid OpenAI-format JSON: {e}"))?;
    if resp.data.is_empty() {
        return Err("embeddings response contained no vectors (empty `data`)".to_string());
    }
    resp.data.sort_by_key(|d| d.index);
    Ok(resp.data.into_iter().map(|d| d.embedding).collect())
}

/// A real embedder that turns text into vectors by calling an OpenAI-compatible `/embeddings` endpoint.
/// Holds its transport, resolved endpoint URL, model name, optional bearer token, and the vector
/// dimensionality discovered at connect time.
pub struct HttpEmbedder {
    transport: Box<dyn EmbeddingsTransport>,
    url: String,
    model: String,
    /// The API key, already trimmed to `None` when blank. Sent only as `Authorization: Bearer …`; never
    /// logged, never returned.
    bearer: Option<String>,
    dim: usize,
}

impl HttpEmbedder {
    /// Build over an **injected** transport (the seam tests use). Resolves the endpoint URL, keeps the
    /// key only if non-blank, then probes once to discover [`dim`](Embedder::dim). Any transport/parse
    /// failure — or an empty probe vector — is a clear `Err` (unreachable endpoint, bad key, bad
    /// response), never a panic.
    pub fn with_transport(
        transport: Box<dyn EmbeddingsTransport>,
        base_url: &str,
        model: &str,
        api_key: Option<String>,
    ) -> Result<Self, String> {
        let bearer = api_key.filter(|k| !k.trim().is_empty());
        let mut e = HttpEmbedder {
            transport,
            url: embeddings_url(base_url),
            model: model.trim().to_string(),
            bearer,
            dim: 0,
        };
        let probe = e.request(&[PROBE_TEXT])?;
        let first = probe
            .into_iter()
            .next()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| "embeddings endpoint returned an empty vector".to_string())?;
        e.dim = first.len();
        Ok(e)
    }

    /// One request→parse round-trip over the transport. Fallible (used by the connect probe and, with a
    /// swallowed error, by the [`Embedder`] impl).
    fn request(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let body = build_request_body(&self.model, texts);
        let raw = self.transport.post(&self.url, self.bearer.as_deref(), &body)?;
        parse_embeddings_response(&raw)
    }

    /// Coerce the model's returned vectors to exactly `texts.len()` rows of exactly `self.dim` columns,
    /// so a persisted [`crate::vector_index`] never sees a dimension mismatch even if the endpoint
    /// hiccups mid-build. A shortfall (fewer rows, short/long vectors) is zero-padded / truncated; a
    /// whole-request failure yields all-zero vectors (that chunk simply won't match), never a panic.
    fn embed_coerced(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        let zero = || vec![0.0f32; self.dim];
        let mut rows = self.request(texts).unwrap_or_default();
        rows.truncate(texts.len());
        while rows.len() < texts.len() {
            rows.push(zero());
        }
        for row in &mut rows {
            row.resize(self.dim, 0.0);
        }
        rows
    }
}

impl Embedder for HttpEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        self.embed_coerced(&[text]).into_iter().next().unwrap_or_else(|| vec![0.0f32; self.dim])
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        self.embed_coerced(texts)
    }
}

// ---------------------------------------------------------------------------
// Real `ureq` transport — feature-gated so the lean default build pulls in no HTTP/TLS stack.
// ---------------------------------------------------------------------------

/// The production transport over `ureq` (blocking HTTP; pure-Rust rustls TLS). Only compiled with the
/// `http-embedder` feature so the default `cpe-server` build carries zero HTTP code.
#[cfg(feature = "http-embedder")]
struct UreqTransport {
    agent: ureq::Agent,
}

#[cfg(feature = "http-embedder")]
impl UreqTransport {
    fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(10))
            // A batch embed of a whole file's chunks can be sizeable; allow a generous read window while
            // still bounding a truly stuck endpoint.
            .timeout(std::time::Duration::from_secs(120))
            .build();
        UreqTransport { agent }
    }
}

#[cfg(feature = "http-embedder")]
impl EmbeddingsTransport for UreqTransport {
    fn post(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<Vec<u8>, String> {
        let mut req = self.agent.post(url).set("Content-Type", "application/json");
        if let Some(token) = bearer {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        // NOTE: the bearer token is never included in any error below (it is a secret).
        match req.send_string(body) {
            Ok(resp) => {
                let mut buf = Vec::new();
                use std::io::Read;
                resp.into_reader()
                    .take(64 * 1024 * 1024)
                    .read_to_end(&mut buf)
                    .map_err(|e| format!("reading embeddings response failed: {e}"))?;
                Ok(buf)
            }
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                let detail = detail.trim();
                if detail.is_empty() {
                    Err(format!("embeddings endpoint returned HTTP {code}"))
                } else {
                    // The response body is the server's own error text (e.g. "invalid api key"); it does
                    // not contain our bearer token. Truncated so a chatty body can't balloon the message.
                    let snippet: String = detail.chars().take(300).collect();
                    Err(format!("embeddings endpoint returned HTTP {code}: {snippet}"))
                }
            }
            Err(ureq::Error::Transport(t)) => Err(format!("could not reach embeddings endpoint: {t}")),
        }
    }
}

/// Connect to a real OpenAI-compatible endpoint over `ureq`, probing once to discover the model's vector
/// dimensionality. `api_key` is `None`/blank for a local server (LM Studio/Ollama) and the bearer token
/// for a hosted one. A clear `Err` on an unreachable endpoint, a bad key, or a malformed response —
/// never a panic. Feature-gated (`http-embedder`).
#[cfg(feature = "http-embedder")]
pub fn connect(base_url: &str, model: &str, api_key: Option<String>) -> Result<HttpEmbedder, String> {
    HttpEmbedder::with_transport(Box::new(UreqTransport::new()), base_url, model, api_key)
}

impl HttpEmbedder {
    /// Convenience wrapper around the free [`connect`] over the real `ureq` transport. Feature-gated.
    #[cfg(feature = "http-embedder")]
    pub fn connect(base_url: &str, model: &str, api_key: Option<String>) -> Result<HttpEmbedder, String> {
        connect(base_url, model, api_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A fake transport: records the last request seen and returns a canned body (or a canned error), so
    /// the whole request→parse→dim flow is exercised with **no network**.
    struct FakeTransport {
        /// The bytes to return for a successful POST.
        response: Vec<u8>,
        /// When set, `post` returns this error instead of `response`.
        error: Option<String>,
        /// Captured (url, bearer, body) of the most recent call, for assertions.
        seen: Mutex<Option<(String, Option<String>, String)>>,
    }

    impl FakeTransport {
        fn ok(body: &str) -> Self {
            FakeTransport { response: body.as_bytes().to_vec(), error: None, seen: Mutex::new(None) }
        }
        fn failing(msg: &str) -> Self {
            FakeTransport { response: Vec::new(), error: Some(msg.to_string()), seen: Mutex::new(None) }
        }
    }

    impl EmbeddingsTransport for FakeTransport {
        fn post(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<Vec<u8>, String> {
            *self.seen.lock().unwrap() =
                Some((url.to_string(), bearer.map(|s| s.to_string()), body.to_string()));
            match &self.error {
                Some(e) => Err(e.clone()),
                None => Ok(self.response.clone()),
            }
        }
    }

    /// A 3-dim embeddings response body, `data` deliberately OUT of index order to prove reordering.
    fn body_3dim() -> String {
        r#"{"object":"list","model":"m","data":[
            {"index":1,"embedding":[0.1,0.2,0.3]},
            {"index":0,"embedding":[1.0,1.0,1.0]}
        ],"usage":{"prompt_tokens":3}}"#
            .to_string()
    }

    #[test]
    fn embeddings_url_accepts_base_with_or_without_v1() {
        assert_eq!(embeddings_url("http://localhost:1234/v1"), "http://localhost:1234/v1/embeddings");
        assert_eq!(embeddings_url("http://localhost:1234/v1/"), "http://localhost:1234/v1/embeddings");
        assert_eq!(embeddings_url("http://localhost:1234"), "http://localhost:1234/v1/embeddings");
        assert_eq!(embeddings_url("https://api.openai.com/v1"), "https://api.openai.com/v1/embeddings");
        // An already-full endpoint is used verbatim.
        assert_eq!(
            embeddings_url("http://host/v1/embeddings"),
            "http://host/v1/embeddings"
        );
        // Whitespace + trailing slash tolerated.
        assert_eq!(embeddings_url("  http://host/v1/  "), "http://host/v1/embeddings");
    }

    #[test]
    fn build_request_body_is_openai_shaped() {
        let body = build_request_body("text-embedding-3-small", &["hello", "world"]);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["model"], "text-embedding-3-small");
        assert_eq!(v["input"][0], "hello");
        assert_eq!(v["input"][1], "world");
    }

    #[test]
    fn parse_reorders_by_index_and_reads_vectors() {
        let vecs = parse_embeddings_response(body_3dim().as_bytes()).unwrap();
        assert_eq!(vecs.len(), 2);
        assert_eq!(vecs[0], vec![1.0, 1.0, 1.0], "index 0 datum comes first after sort");
        assert_eq!(vecs[1], vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn parse_rejects_garbage_and_empty_data_without_panicking() {
        assert!(parse_embeddings_response(b"not json").is_err());
        assert!(parse_embeddings_response(br#"{"data":[]}"#).is_err());
        // Missing `data` key entirely.
        assert!(parse_embeddings_response(br#"{"object":"list"}"#).is_err());
    }

    #[test]
    fn connect_probes_dim_and_sends_bearer_when_keyed() {
        // Share the fake behind an Arc so the test can inspect what the embedder actually sent.
        let fake = std::sync::Arc::new(FakeTransport::ok(&body_3dim()));
        let e = HttpEmbedder::with_transport(
            Box::new(ArcTransport(fake.clone())),
            "http://host/v1",
            "my-model",
            Some("sk-secret".to_string()),
        )
        .unwrap();
        assert_eq!(e.dim(), 3, "dim comes from the probe vector length");

        // A keyed embed sends the bearer + posts to the resolved URL.
        let v = e.embed("hello");
        assert_eq!(v.len(), 3);
        let seen = fake.seen.lock().unwrap().clone();
        let (url, bearer, _body) = seen.unwrap();
        assert_eq!(url, "http://host/v1/embeddings");
        assert_eq!(bearer.as_deref(), Some("sk-secret"));
    }

    /// Lets a test share one `FakeTransport` behind an `Arc` so it can inspect what the embedder sent.
    struct ArcTransport(std::sync::Arc<FakeTransport>);
    impl EmbeddingsTransport for ArcTransport {
        fn post(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<Vec<u8>, String> {
            self.0.post(url, bearer, body)
        }
    }

    #[test]
    fn no_key_means_no_authorization_header() {
        let fake = std::sync::Arc::new(FakeTransport::ok(&body_3dim()));
        let e = HttpEmbedder::with_transport(
            Box::new(ArcTransport(fake.clone())),
            "http://localhost:1234/v1",
            "local-model",
            None,
        )
        .unwrap();
        let _ = e.embed("hi");
        let (_url, bearer, _body) = fake.seen.lock().unwrap().clone().unwrap();
        assert_eq!(bearer, None, "a local server gets no bearer header");

        // A blank/whitespace key is treated as no key.
        let fake2 = std::sync::Arc::new(FakeTransport::ok(&body_3dim()));
        let e2 = HttpEmbedder::with_transport(
            Box::new(ArcTransport(fake2.clone())),
            "http://localhost:1234/v1",
            "local-model",
            Some("   ".to_string()),
        )
        .unwrap();
        let _ = e2.embed("hi");
        assert_eq!(fake2.seen.lock().unwrap().clone().unwrap().1, None);
    }

    #[test]
    fn connect_error_never_leaks_the_key() {
        let err = HttpEmbedder::with_transport(
            Box::new(FakeTransport::failing("could not reach embeddings endpoint: connection refused")),
            "http://host/v1",
            "m",
            Some("sk-topsecret".to_string()),
        )
        .err()
        .expect("a failing transport must yield an Err");
        assert!(!err.contains("sk-topsecret"), "the API key must never appear in an error: {err}");
    }

    #[test]
    fn embed_coerces_rows_and_dims_and_never_panics_on_failure() {
        // dim 3 probed from a good response…
        let good = body_3dim();
        // …then a mid-build request that fails: embed_batch must return zero vectors of the right shape.
        struct FirstOkThenFail {
            good: Vec<u8>,
            calls: Mutex<u32>,
        }
        impl EmbeddingsTransport for FirstOkThenFail {
            fn post(&self, _u: &str, _b: Option<&str>, _body: &str) -> Result<Vec<u8>, String> {
                let mut n = self.calls.lock().unwrap();
                *n += 1;
                if *n == 1 {
                    Ok(self.good.clone())
                } else {
                    Err("endpoint went away".to_string())
                }
            }
        }
        let e = HttpEmbedder::with_transport(
            Box::new(FirstOkThenFail { good: good.into_bytes(), calls: Mutex::new(0) }),
            "http://host/v1",
            "m",
            None,
        )
        .unwrap();
        assert_eq!(e.dim(), 3);
        // Second call (the batch) fails inside the transport → all-zero vectors of dim 3, one per input.
        let out = e.embed_batch(&["a", "b"]);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|v| v.len() == 3 && v.iter().all(|&x| x == 0.0)));

        // Empty input never hits the network and returns nothing.
        assert!(e.embed_batch(&[]).is_empty());
    }

    #[test]
    fn short_or_long_vectors_are_coerced_to_dim() {
        // Probe establishes dim 3; then a response with a too-short and a too-long vector is coerced.
        let e = HttpEmbedder::with_transport(
            Box::new(SwapTransport::new(
                &body_3dim(),
                r#"{"data":[{"index":0,"embedding":[9.0]},{"index":1,"embedding":[1.0,2.0,3.0,4.0,5.0]}]}"#,
            )),
            "http://host/v1",
            "m",
            None,
        )
        .unwrap();
        let out = e.embed_batch(&["x", "y"]);
        assert_eq!(out[0], vec![9.0, 0.0, 0.0], "short vector zero-padded to dim");
        assert_eq!(out[1], vec![1.0, 2.0, 3.0], "long vector truncated to dim");
    }

    /// Returns `first` on the initial (probe) call, then `second` on every subsequent call.
    struct SwapTransport {
        first: Vec<u8>,
        second: Vec<u8>,
        calls: Mutex<u32>,
    }
    impl SwapTransport {
        fn new(first: &str, second: &str) -> Self {
            SwapTransport {
                first: first.as_bytes().to_vec(),
                second: second.as_bytes().to_vec(),
                calls: Mutex::new(0),
            }
        }
    }
    impl EmbeddingsTransport for SwapTransport {
        fn post(&self, _u: &str, _b: Option<&str>, _body: &str) -> Result<Vec<u8>, String> {
            let mut n = self.calls.lock().unwrap();
            *n += 1;
            Ok(if *n == 1 { self.first.clone() } else { self.second.clone() })
        }
    }
}
