//! AI **file-copilot LLM seam** (CPE-1275, epic CPE-977): the natural-language → [`FileOpPlan`]
//! translator. This is slice 1's *risky* half — an LLM proposes file operations — so it is built
//! defensively and, crucially, its output is only ever a **closed, whitelisted** [`crate::op_plan::
//! FileOpPlan`] (move/rename/delete/mkdir/copy) by construction: there is no free-form or shell escape,
//! and every candidate plan is validated + human-confirmed before [`crate::copilot`] touches disk.
//!
//! # Why this shape (mirrors [`crate::http_embedder`], CPE-1273)
//! The connection is an **OpenAI-compatible** `/chat/completions` call over **`ureq`** — already a
//! workspace dependency, so no new HTTP stack enters the tree — feature-gated (`copilot`) so the lean
//! default build compiles zero HTTP/TLS code. The model is asked (via [`build_system_prompt`]) to emit
//! ONLY a JSON `FileOpPlan` over the closed op set, using the model's JSON/structured-output mode; the
//! response is parsed **robustly** ([`parse_plan_from_content`]) — a valid plan or a clear `Err`, never a
//! panic and never a partial execution.
//!
//! # Testable without a network
//! The transport is a seam ([`ChatTransport`]): the pure prompt-building ([`build_system_prompt`] /
//! [`build_user_prompt`]), request-serialisation ([`build_request_body`]), URL-normalisation
//! ([`chat_completions_url`]), and the two response parsers ([`parse_chat_content`] +
//! [`parse_plan_from_content`]) are plain functions with unit tests, and [`HttpPlanner`] drives the whole
//! instruction→plan path over an **injected** transport, so it is exercised headlessly with a fake — no
//! real network in any test. A [`FakePlanner`] (deterministic, no network) backs the command-layer tests.
//! The real `ureq` transport ([`connect`]) is feature-gated; end-to-end quality is the user's own model.
//!
//! # Never panics, never leaks the key
//! Every failure (unreachable endpoint, bad key, malformed body, non-plan output) maps to a clear `Err`.
//! The API key travels only in the `Authorization` header and is **never** placed in an error message, a
//! log line, or any returned value — identical to the embedder's discipline.

use serde::{Deserialize, Serialize};

use crate::op_plan::FileOpPlan;

/// One direct child of the target folder handed to the model as context: its bare `name` and whether it
/// is a directory. Deliberately minimal — the model needs the tree shape to plan against, not sizes or
/// timestamps, and a smaller prompt is cheaper + less error-prone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlanEntry {
    pub name: String,
    pub is_dir: bool,
}

/// The copilot's planner seam: turn a natural-language `instruction` over the folder `root` (whose direct
/// children are `entries`) into a candidate [`FileOpPlan`]. Implemented by the real [`HttpPlanner`] and the
/// deterministic [`FakePlanner`].
///
/// Contract: **never panics**. Any failure — an unreachable/erroring model, an unparseable response, output
/// that is not a valid `FileOpPlan` — is a clear `Err`. The returned plan is *unvalidated*: the caller
/// ([`crate::copilot`]) runs [`crate::op_plan::validate`] against the scope envelope before anything is
/// previewed or executed.
pub trait LlmPlanner: Send + Sync {
    fn plan(&self, root: &str, instruction: &str, entries: &[PlanEntry]) -> Result<FileOpPlan, String>;
}

/// The transport seam: POST a JSON `body` to `url` with an optional bearer token, returning the raw
/// response bytes or a legible error. Abstracted off [`HttpPlanner`] so the request/parse flow is
/// unit-testable with a fake (no real network); the production impl over `ureq` is feature-gated.
///
/// Contract: the `bearer` token MUST NOT appear in the returned error string (it is a secret).
pub trait ChatTransport: Send + Sync {
    fn post(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<Vec<u8>, String>;
}

/// Normalise a user-supplied base URL into the full `/chat/completions` endpoint, accepting it **with or
/// without** the `/v1` segment — identical policy to [`crate::http_embedder::embeddings_url`]:
///
/// - `http://host:1234/v1/chat/completions` → used as-is.
/// - `http://host:1234/v1`                  → `…/v1/chat/completions` (LM Studio's own base URL shape).
/// - `http://host:1234`                     → `…/v1/chat/completions` (bare host).
/// - `https://api.openai.com/v1`            → `…/v1/chat/completions`.
pub fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

/// The system prompt: instruct the model to output ONLY a JSON [`FileOpPlan`] over the closed op set, with
/// every path an absolute path inside `root`. The op shapes described here match the crate's serde wire
/// form exactly (snake_case tags), so a compliant model's output deserialises directly.
pub fn build_system_prompt(root: &str) -> String {
    format!(
        "You are a careful file-management planner for a desktop file explorer. Given a user's request \
and a listing of one folder, output a PLAN of concrete file operations to fulfil it.\n\n\
STRICT OUTPUT RULES:\n\
- Respond with ONLY a single JSON object, no prose, no markdown fences.\n\
- The object has exactly one key, \"ops\": an ordered array of operations, applied top to bottom.\n\
- Each operation is exactly ONE of these shapes (and NOTHING else):\n\
  {{\"move\":{{\"src\":\"<abs path>\",\"dst\":\"<abs path>\"}}}}\n\
  {{\"copy\":{{\"src\":\"<abs path>\",\"dst\":\"<abs path>\"}}}}\n\
  {{\"rename\":{{\"path\":\"<abs path>\",\"new_name\":\"<bare filename>\"}}}}\n\
  {{\"mkdir\":{{\"path\":\"<abs path>\"}}}}\n\
  {{\"delete\":{{\"path\":\"<abs path>\"}}}}\n\
- EVERY path MUST be an absolute path located inside this root folder: {root}\n\
- Never propose a path outside the root, never use \"..\" to climb out of it.\n\
- \"new_name\" for a rename is a bare filename only (no slashes, no \"..\").\n\
- Only use the five operations above. There is no shell, no arbitrary command, no other op.\n\
- If the request cannot be satisfied safely, return {{\"ops\":[]}}.\n\
Return only the JSON object."
    )
}

/// The user prompt: the natural-language `instruction` plus the folder `root` and its direct children
/// (`entries`), one per line as `[DIR] name` / `[FILE] name`, so the model can plan against real names.
pub fn build_user_prompt(root: &str, instruction: &str, entries: &[PlanEntry]) -> String {
    let mut listing = String::new();
    for e in entries {
        let kind = if e.is_dir { "[DIR] " } else { "[FILE]" };
        listing.push_str(kind);
        listing.push(' ');
        listing.push_str(&e.name);
        listing.push('\n');
    }
    if listing.is_empty() {
        listing.push_str("(the folder is empty)\n");
    }
    format!(
        "Target folder (root): {root}\n\nDirect children of the folder:\n{listing}\n\
Request: {instruction}\n\nReturn only the JSON plan object."
    )
}

// The `/chat/completions` request body (`{ model, messages, temperature, response_format }`) — the
// OpenAI-compatible shape every target server accepts. `temperature: 0` for the most deterministic plan;
// `response_format: {"type":"json_object"}` asks a supporting server for guaranteed-JSON output (ignored
// harmlessly by servers that don't support it, since we parse robustly regardless).
#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    response_format: ResponseFormat,
}

/// Serialise the chat request body for `model` + `system`/`user` prompts — pure, so it's unit-tested
/// directly. A serialisation failure (can't happen for borrowed data) falls back to a minimal valid body
/// rather than panicking.
pub fn build_request_body(model: &str, system: &str, user: &str) -> String {
    let req = ChatRequest {
        model,
        messages: vec![
            ChatMessage { role: "system", content: system },
            ChatMessage { role: "user", content: user },
        ],
        temperature: 0.0,
        response_format: ResponseFormat { kind: "json_object" },
    };
    serde_json::to_string(&req).unwrap_or_else(|_| {
        format!(
            r#"{{"model":{},"messages":[],"temperature":0}}"#,
            serde_json::to_string(model).unwrap_or_else(|_| "\"\"".into())
        )
    })
}

/// One choice of a chat-completions response. Extra fields (`finish_reason`, `index`, …) are ignored.
#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageOwned,
}

#[derive(Debug, Deserialize)]
struct ChatMessageOwned {
    #[serde(default)]
    content: String,
}

/// The chat-completions response envelope we read: just its `choices` array.
#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

/// Parse an OpenAI-format chat-completions response body into the first choice's message content. A
/// malformed/empty body — or an empty content string — is a clear `Err`, never a panic.
pub fn parse_chat_content(bytes: &[u8]) -> Result<String, String> {
    let resp: ChatResponse = serde_json::from_slice(bytes)
        .map_err(|e| format!("model response was not valid OpenAI chat JSON: {e}"))?;
    let content = resp
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .ok_or_else(|| "model response contained no choices".to_string())?;
    if content.trim().is_empty() {
        return Err("model returned an empty message".to_string());
    }
    Ok(content)
}

/// A `{ "plan": FileOpPlan }` wrapper — some models nest the plan under a `plan` key despite the prompt.
/// Tolerated by [`parse_plan_from_content`] as a fallback so a well-formed-but-wrapped plan still parses.
#[derive(Debug, Deserialize)]
struct PlanWrapper {
    plan: FileOpPlan,
}

/// Extract the outermost JSON object substring from `s` (first `{` … matching last `}`), so a model that
/// wraps its JSON in markdown fences or adds a sentence around it still yields the object. Returns `None`
/// when there is no `{`…`}` pair. `{` and `}` are ASCII, so byte indexing is safe.
fn extract_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end > start {
        Some(&s[start..=end])
    } else {
        None
    }
}

/// Robustly parse a model's message `content` into a [`FileOpPlan`]. Tries, in order: the content verbatim
/// as a `FileOpPlan`; as a `{ "plan": … }` wrapper; then the same two against the outermost `{…}` substring
/// (handles fences/prose). Anything that is not a valid `FileOpPlan` — garbage, a bare `{}`, an op outside
/// the closed set — is a clear `Err`, never a panic and never a partial plan. The op whitelist is enforced
/// by [`FileOpPlan`]'s own closed enum: an unknown op tag simply fails to deserialise.
pub fn parse_plan_from_content(content: &str) -> Result<FileOpPlan, String> {
    let trimmed = content.trim();
    let attempts: [&str; 2] = [trimmed, extract_json_object(trimmed).unwrap_or("")];
    for candidate in attempts {
        if candidate.is_empty() {
            continue;
        }
        if let Ok(plan) = serde_json::from_str::<FileOpPlan>(candidate) {
            return Ok(plan);
        }
        if let Ok(wrapper) = serde_json::from_str::<PlanWrapper>(candidate) {
            return Ok(wrapper.plan);
        }
    }
    Err("model output was not a valid file-operation plan (expected a JSON object with an \"ops\" array)"
        .to_string())
}

/// A real planner that turns an instruction into a [`FileOpPlan`] by calling an OpenAI-compatible
/// `/chat/completions` endpoint. Holds its transport, the resolved endpoint URL, the model name, and an
/// optional bearer token (sent only in the `Authorization` header; never logged, never returned).
pub struct HttpPlanner {
    transport: Box<dyn ChatTransport>,
    url: String,
    model: String,
    bearer: Option<String>,
}

impl HttpPlanner {
    /// Build over an **injected** transport (the seam tests use). Resolves the endpoint URL and keeps the
    /// key only if non-blank. Construction is infallible — unlike the embedder there is no connect-time
    /// probe; any transport/model failure surfaces at [`LlmPlanner::plan`] time as a clear `Err`.
    pub fn with_transport(
        transport: Box<dyn ChatTransport>,
        base_url: &str,
        model: &str,
        api_key: Option<String>,
    ) -> Self {
        HttpPlanner {
            transport,
            url: chat_completions_url(base_url),
            model: model.trim().to_string(),
            bearer: api_key.filter(|k| !k.trim().is_empty()),
        }
    }
}

impl LlmPlanner for HttpPlanner {
    fn plan(&self, root: &str, instruction: &str, entries: &[PlanEntry]) -> Result<FileOpPlan, String> {
        let system = build_system_prompt(root);
        let user = build_user_prompt(root, instruction, entries);
        let body = build_request_body(&self.model, &system, &user);
        let raw = self.transport.post(&self.url, self.bearer.as_deref(), &body)?;
        let content = parse_chat_content(&raw)?;
        parse_plan_from_content(&content)
    }
}

/// A deterministic, network-free planner for tests + the command-layer pipeline: it returns a preset
/// result regardless of the instruction, so the safety chain (validate → checkpoint → trash-delete → undo)
/// is exercised end-to-end with no LLM. Construct with [`FakePlanner::returning`] (a canned plan) or
/// [`FakePlanner::failing`] (simulate an unreachable/erroring model).
pub struct FakePlanner {
    result: Result<FileOpPlan, String>,
}

impl FakePlanner {
    /// A planner that always returns `plan`.
    pub fn returning(plan: FileOpPlan) -> Self {
        FakePlanner { result: Ok(plan) }
    }
    /// A planner that always fails with `msg` (models unreachable / bad output).
    pub fn failing(msg: impl Into<String>) -> Self {
        FakePlanner { result: Err(msg.into()) }
    }
}

impl LlmPlanner for FakePlanner {
    fn plan(&self, _root: &str, _instruction: &str, _entries: &[PlanEntry]) -> Result<FileOpPlan, String> {
        self.result.clone()
    }
}

// ---------------------------------------------------------------------------
// Real `ureq` transport — feature-gated (`copilot`) so the lean default build pulls in no HTTP/TLS stack.
// ---------------------------------------------------------------------------

/// The production transport over `ureq` (blocking HTTP; pure-Rust rustls TLS). Only compiled with the
/// `copilot` feature so the default `cpe-server` build carries zero HTTP code. Mirrors
/// [`crate::http_embedder`]'s `UreqTransport`.
#[cfg(feature = "copilot")]
struct UreqChatTransport {
    agent: ureq::Agent,
}

#[cfg(feature = "copilot")]
impl UreqChatTransport {
    fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(10))
            // A model generating a plan can take a while; allow a generous read window while still
            // bounding a truly stuck endpoint.
            .timeout(std::time::Duration::from_secs(180))
            .build();
        UreqChatTransport { agent }
    }
}

#[cfg(feature = "copilot")]
impl ChatTransport for UreqChatTransport {
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
                    .take(16 * 1024 * 1024)
                    .read_to_end(&mut buf)
                    .map_err(|e| format!("reading model response failed: {e}"))?;
                Ok(buf)
            }
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp.into_string().unwrap_or_default();
                let detail = detail.trim();
                if detail.is_empty() {
                    Err(format!("model endpoint returned HTTP {code}"))
                } else {
                    // The response body is the server's own error text; it does not contain our bearer
                    // token. Truncated so a chatty body can't balloon the message.
                    let snippet: String = detail.chars().take(300).collect();
                    Err(format!("model endpoint returned HTTP {code}: {snippet}"))
                }
            }
            Err(ureq::Error::Transport(t)) => Err(format!("could not reach model endpoint: {t}")),
        }
    }
}

/// Connect to a real OpenAI-compatible `/chat/completions` endpoint over `ureq`. `api_key` is `None`/blank
/// for a local server (LM Studio/Ollama) and the bearer token for a hosted one. Construction is infallible;
/// failures surface at [`LlmPlanner::plan`] time. Feature-gated (`copilot`).
#[cfg(feature = "copilot")]
pub fn connect(base_url: &str, model: &str, api_key: Option<String>) -> HttpPlanner {
    HttpPlanner::with_transport(Box::new(UreqChatTransport::new()), base_url, model, api_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op_plan::FileOp;
    use std::sync::Mutex;

    /// A fake transport: records the last request seen and returns a canned body (or a canned error), so
    /// the whole plan-request→parse flow is exercised with **no network**.
    struct FakeTransport {
        response: Vec<u8>,
        error: Option<String>,
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
    impl ChatTransport for FakeTransport {
        fn post(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<Vec<u8>, String> {
            *self.seen.lock().unwrap() =
                Some((url.to_string(), bearer.map(|s| s.to_string()), body.to_string()));
            match &self.error {
                Some(e) => Err(e.clone()),
                None => Ok(self.response.clone()),
            }
        }
    }

    /// A chat response whose message content is `content` (as the server would return it).
    fn chat_body(content: &str) -> String {
        serde_json::json!({
            "choices": [ { "message": { "role": "assistant", "content": content } } ]
        })
        .to_string()
    }

    #[test]
    fn chat_url_accepts_base_with_or_without_v1() {
        assert_eq!(chat_completions_url("http://localhost:1234/v1"), "http://localhost:1234/v1/chat/completions");
        assert_eq!(chat_completions_url("http://localhost:1234/v1/"), "http://localhost:1234/v1/chat/completions");
        assert_eq!(chat_completions_url("http://localhost:1234"), "http://localhost:1234/v1/chat/completions");
        assert_eq!(chat_completions_url("https://api.openai.com/v1"), "https://api.openai.com/v1/chat/completions");
        assert_eq!(chat_completions_url("http://h/v1/chat/completions"), "http://h/v1/chat/completions");
        assert_eq!(chat_completions_url("  http://h/v1/  "), "http://h/v1/chat/completions");
    }

    #[test]
    fn request_body_is_openai_chat_shaped() {
        let body = build_request_body("m", "sys", "usr");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["model"], "m");
        assert_eq!(v["messages"][0]["role"], "system");
        assert_eq!(v["messages"][0]["content"], "sys");
        assert_eq!(v["messages"][1]["role"], "user");
        assert_eq!(v["messages"][1]["content"], "usr");
        assert_eq!(v["response_format"]["type"], "json_object");
    }

    #[test]
    fn user_prompt_lists_entries_and_handles_empty() {
        let entries = vec![
            PlanEntry { name: "a.txt".into(), is_dir: false },
            PlanEntry { name: "sub".into(), is_dir: true },
        ];
        let p = build_user_prompt("/root", "tidy up", &entries);
        assert!(p.contains("/root"));
        assert!(p.contains("[FILE] a.txt"));
        assert!(p.contains("[DIR]  sub"));
        assert!(p.contains("tidy up"));
        let empty = build_user_prompt("/root", "x", &[]);
        assert!(empty.contains("(the folder is empty)"));
    }

    #[test]
    fn parse_plan_accepts_plain_object() {
        let plan = parse_plan_from_content(r#"{"ops":[{"mkdir":{"path":"/root/A"}}]}"#).unwrap();
        assert_eq!(plan.ops, vec![FileOp::Mkdir { path: "/root/A".into() }]);
    }

    #[test]
    fn parse_plan_extracts_object_from_fenced_prose() {
        let content = "Sure! Here is the plan:\n```json\n{\"ops\":[{\"delete\":{\"path\":\"/r/x\"}}]}\n```\nDone.";
        let plan = parse_plan_from_content(content).unwrap();
        assert_eq!(plan.ops, vec![FileOp::Delete { path: "/r/x".into() }]);
    }

    #[test]
    fn parse_plan_accepts_plan_wrapper() {
        let plan = parse_plan_from_content(r#"{"plan":{"ops":[]},"notes":"nothing to do"}"#).unwrap();
        assert!(plan.ops.is_empty());
    }

    #[test]
    fn parse_plan_rejects_garbage_without_panicking() {
        assert!(parse_plan_from_content("not json at all").is_err());
        assert!(parse_plan_from_content("").is_err());
        assert!(parse_plan_from_content("{}").is_err()); // no `ops` key
        // An op OUTSIDE the closed whitelist can't deserialise into the closed enum.
        assert!(parse_plan_from_content(r#"{"ops":[{"exec":{"cmd":"rm -rf /"}}]}"#).is_err());
        // Truncated / half object.
        assert!(parse_plan_from_content(r#"{"ops":[{"move":{"src":"#).is_err());
    }

    #[test]
    fn parse_chat_content_reads_first_choice_and_rejects_empty() {
        let c = parse_chat_content(chat_body("hello").as_bytes()).unwrap();
        assert_eq!(c, "hello");
        assert!(parse_chat_content(b"not json").is_err());
        assert!(parse_chat_content(br#"{"choices":[]}"#).is_err());
        assert!(parse_chat_content(&chat_body("   ").into_bytes()).is_err());
    }

    #[test]
    fn http_planner_full_flow_over_fake_transport() {
        let content = r#"{"ops":[{"mkdir":{"path":"/root/Archive"}},{"move":{"src":"/root/a.txt","dst":"/root/Archive/a.txt"}}]}"#;
        let fake = FakeTransport::ok(&chat_body(content));
        let planner = HttpPlanner::with_transport(
            Box::new(ArcTransport(std::sync::Arc::new(fake))),
            "http://host/v1",
            "my-model",
            Some("sk-secret".into()),
        );
        let plan = planner.plan("/root", "archive a.txt", &[]).unwrap();
        assert_eq!(plan.ops.len(), 2);
    }

    #[test]
    fn http_planner_sends_bearer_and_resolved_url() {
        let fake = std::sync::Arc::new(FakeTransport::ok(&chat_body(r#"{"ops":[]}"#)));
        let planner = HttpPlanner::with_transport(
            Box::new(ArcTransport(fake.clone())),
            "http://host/v1",
            "m",
            Some("sk-secret".into()),
        );
        let _ = planner.plan("/root", "x", &[]).unwrap();
        let (url, bearer, _body) = fake.seen.lock().unwrap().clone().unwrap();
        assert_eq!(url, "http://host/v1/chat/completions");
        assert_eq!(bearer.as_deref(), Some("sk-secret"));
    }

    #[test]
    fn http_planner_no_key_means_no_bearer() {
        let fake = std::sync::Arc::new(FakeTransport::ok(&chat_body(r#"{"ops":[]}"#)));
        let planner =
            HttpPlanner::with_transport(Box::new(ArcTransport(fake.clone())), "http://h/v1", "m", None);
        let _ = planner.plan("/root", "x", &[]).unwrap();
        assert_eq!(fake.seen.lock().unwrap().clone().unwrap().1, None);
        // A blank key is treated as no key.
        let fake2 = std::sync::Arc::new(FakeTransport::ok(&chat_body(r#"{"ops":[]}"#)));
        let p2 = HttpPlanner::with_transport(
            Box::new(ArcTransport(fake2.clone())),
            "http://h/v1",
            "m",
            Some("   ".into()),
        );
        let _ = p2.plan("/root", "x", &[]).unwrap();
        assert_eq!(fake2.seen.lock().unwrap().clone().unwrap().1, None);
    }

    #[test]
    fn http_planner_transport_error_never_leaks_key() {
        let planner = HttpPlanner::with_transport(
            Box::new(FakeTransport::failing("could not reach model endpoint: connection refused")),
            "http://h/v1",
            "m",
            Some("sk-topsecret".into()),
        );
        let err = planner.plan("/root", "x", &[]).unwrap_err();
        assert!(!err.contains("sk-topsecret"), "the API key must never appear in an error: {err}");
    }

    #[test]
    fn http_planner_bad_model_output_is_err_not_panic() {
        let planner = HttpPlanner::with_transport(
            Box::new(FakeTransport::ok(&chat_body("I refuse to answer in JSON."))),
            "http://h/v1",
            "m",
            None,
        );
        assert!(planner.plan("/root", "x", &[]).is_err());
    }

    #[test]
    fn fake_planner_returns_and_fails() {
        let p = FakePlanner::returning(FileOpPlan { ops: vec![FileOp::Mkdir { path: "/r/A".into() }] });
        assert_eq!(p.plan("/r", "x", &[]).unwrap().ops.len(), 1);
        let f = FakePlanner::failing("model down");
        assert_eq!(f.plan("/r", "x", &[]).unwrap_err(), "model down");
    }

    /// Lets a test share one `FakeTransport` behind an `Arc` to inspect what the planner sent.
    struct ArcTransport(std::sync::Arc<FakeTransport>);
    impl ChatTransport for ArcTransport {
        fn post(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<Vec<u8>, String> {
            self.0.post(url, bearer, body)
        }
    }
}
