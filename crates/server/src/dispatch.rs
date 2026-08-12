//! Server-side contract dispatch (CPE-824, epic CPE-810): turn a [`Request`] envelope into a
//! [`Response`] by looking the method up in a registry and calling the matching `cpe-server` domain
//! function. This is what a network `Client(Rust)` drives over a socket in CPE-820 — here with **no
//! transport**, so it's fully unit-testable. Adding a method = register a handler, no core changes.
//!
//! Error taxonomy at the boundary: an unknown method → [`ErrorCode::NotFound`], params that don't
//! deserialize → [`ErrorCode::BadRequest`]; a domain `Err(String)` from a **path-taking** handler goes
//! through [`domain_path`], which is `NotFound` when the path genuinely doesn't exist and `Internal`
//! otherwise (including when the path's existence can't even be determined, e.g. a permission-denied
//! parent-directory traversal — "we don't know" must never be reported as "it isn't there"); every other
//! domain `Err(String)` → [`ErrorCode::Internal`] via [`domain`]. A handler never panics the dispatcher.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::contract::{ContractError, ErrorCode, Request, Response};
use crate::ctx::ServerCtx;

/// A registered method handler: given the runtime context + the request's JSON `params`, produce a JSON
/// result or a structured [`ContractError`].
pub type Handler =
    Box<dyn Fn(&dyn ServerCtx, serde_json::Value) -> Result<serde_json::Value, ContractError> + Send + Sync>;

/// Deserialize a handler's params from the request `Value`, mapping a shape mismatch to `BadRequest`.
pub fn params<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, ContractError> {
    serde_json::from_value(value)
        .map_err(|e| ContractError::new(ErrorCode::BadRequest, format!("invalid params: {e}"), false))
}

/// Serialize a handler's result, mapping a serialization failure to `Internal`.
pub fn result<T: Serialize>(value: T) -> Result<serde_json::Value, ContractError> {
    serde_json::to_value(value)
        .map_err(|e| ContractError::new(ErrorCode::Internal, format!("serialize failed: {e}"), false))
}

/// Map a domain `Err(String)` (e.g. "not a folder") onto a structured contract error.
pub fn domain(err: String) -> ContractError {
    ContractError::new(ErrorCode::Internal, err, false)
}

/// Map a domain `Err(String)` for a **path-taking** handler onto a structured contract error: a
/// genuinely missing `path` is `NotFound` (the one domain failure a caller most needs to branch on
/// structurally — CPE-1659, first applied to `list_dir` only); every other handler that takes a `path`
/// goes through this same helper instead of re-deriving the check, so the mapping is one taxonomy
/// instead of a one-off (CPE-1673 follow-up).
pub fn domain_path(path: &str, err: String) -> ContractError {
    classify_path_error(std::fs::metadata(path).err().map(|e| e.kind()), err)
}

/// The pure classification `domain_path` delegates to, split out so the EACCES-vs-missing distinction is
/// unit-testable without touching the real filesystem (permission bits are platform- and
/// privilege-dependent — e.g. inert when the test process runs as root — so a real `chmod`-based test
/// would be flaky; this stays deterministic on every OS and CI account).
///
/// Deliberately does **not** collapse to `Path::exists()`, which swallows every `stat` failure — missing
/// path AND permission-denied parent-directory traversal (EACCES) alike — into the same `false`. That
/// would report "we don't know" as "it isn't there": only a `stat` that fails with
/// `io::ErrorKind::NotFound` is a genuine `NotFound`; anything else (including `PermissionDenied`, or no
/// error at all because the path exists but the domain call failed for an unrelated reason) stays
/// `Internal`.
fn classify_path_error(stat_err_kind: Option<std::io::ErrorKind>, err: String) -> ContractError {
    match stat_err_kind {
        Some(std::io::ErrorKind::NotFound) => ContractError::new(ErrorCode::NotFound, err, false),
        _ => domain(err),
    }
}

/// The method registry. Look up by name; the missing case is a structural `NotFound` (you can't
/// accidentally dispatch to nothing).
#[derive(Default)]
pub struct Dispatcher {
    handlers: BTreeMap<String, Handler>,
}

impl Dispatcher {
    /// An empty dispatcher.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a handler for `method`.
    pub fn register<F>(&mut self, method: impl Into<String>, handler: F)
    where
        F: Fn(&dyn ServerCtx, serde_json::Value) -> Result<serde_json::Value, ContractError>
            + Send
            + Sync
            + 'static,
    {
        self.handlers.insert(method.into(), Box::new(handler));
    }

    /// The registered method names, sorted.
    pub fn methods(&self) -> Vec<&str> {
        self.handlers.keys().map(String::as_str).collect()
    }

    /// Dispatch one request to its handler and produce a response. An unknown method is a `NotFound`
    /// response, not an error to the caller.
    pub fn dispatch(&self, ctx: &dyn ServerCtx, req: Request) -> Response {
        match self.handlers.get(&req.method) {
            Some(handler) => Response {
                result: handler(ctx, req.params),
            },
            None => Response {
                result: Err(ContractError::new(
                    ErrorCode::NotFound,
                    format!("unknown method: {}", req.method),
                    false,
                )),
            },
        }
    }

    /// A dispatcher pre-seeded with a representative set of methods, proving the pattern across a
    /// no-arg / path-arg / ctx-using / multi-arg handler. The full ~113-method surface is completed with
    /// the typed bindings (CPE-812) so method names stay a single source of truth.
    pub fn with_builtins() -> Self {
        let mut d = Self::new();

        // Path-arg, no ctx: list a directory.
        d.register("list_dir", |_ctx, p| {
            #[derive(serde::Deserialize)]
            struct P {
                path: String,
            }
            let a: P = params(p)?;
            // CPE-1659 / CPE-1673: a missing path is a structured `NotFound` over the wire, not the
            // generic `Internal` every other domain error gets — the real-server rig's Slice 2 E2E test
            // (`crates/net/tests/real_server_e2e.rs`) asserts exactly this ("a missing remote path must
            // be a clean error ... not a panic, not a silent success") and caught this gap for real.
            // `domain_path` applies the same mapping to every path-taking handler (not just this one).
            let entries = crate::listing::list_dir(&a.path).map_err(|e| domain_path(&a.path, e))?;
            result(entries)
        });

        // Path-arg: hash a file.
        d.register("hash_file", |_ctx, p| {
            #[derive(serde::Deserialize)]
            struct P {
                path: String,
            }
            let a: P = params(p)?;
            result(crate::checksum::hash_file(&a.path).map_err(|e| domain_path(&a.path, e))?)
        });

        // Path-arg: text statistics.
        d.register("text_stats", |_ctx, p| {
            #[derive(serde::Deserialize)]
            struct P {
                path: String,
            }
            let a: P = params(p)?;
            result(crate::text_stats::compute(&a.path).map_err(|e| domain_path(&a.path, e))?)
        });

        // No-arg but ctx-using: the whole tag store (resolves the config dir via the ctx).
        d.register("tags.load", |ctx, _p| result(crate::tags::load(ctx).map_err(domain)?));

        // Multi-arg + ctx: set a path's tags/label.
        d.register("tags.set", |ctx, p| {
            #[derive(serde::Deserialize)]
            struct P {
                path: String,
                tags: Vec<String>,
                label: String,
            }
            let a: P = params(p)?;
            result(crate::tags::set(ctx, &a.path, a.tags, a.label).map_err(domain)?)
        });

        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::HeadlessCtx;
    use serde_json::json;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-dispatch-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn req(method: &str, params: serde_json::Value) -> Request {
        Request { method: method.to_string(), params }
    }

    #[test]
    fn dispatch_list_dir_returns_entries() {
        let d = scratch("list");
        std::fs::write(d.join("a.txt"), b"hi").unwrap();
        let ctx = HeadlessCtx::new(scratch("base"));
        let resp = Dispatcher::with_builtins().dispatch(&ctx, req("list_dir", json!({ "path": d.to_string_lossy() })));
        let val = resp.result.expect("list_dir should succeed");
        let names: Vec<String> = val.as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_string()).collect();
        assert!(names.iter().any(|n| n == "a.txt"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn unknown_method_is_not_found() {
        let ctx = HeadlessCtx::new(scratch("base"));
        let resp = Dispatcher::with_builtins().dispatch(&ctx, req("does_not_exist", json!({})));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::NotFound),
            Ok(_) => panic!("unknown method must be NotFound"),
        }
    }

    #[test]
    fn bad_params_are_a_bad_request() {
        let ctx = HeadlessCtx::new(scratch("base"));
        // list_dir needs { path }; send the wrong shape.
        let resp = Dispatcher::with_builtins().dispatch(&ctx, req("list_dir", json!({ "wrong": 1 })));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::BadRequest),
            Ok(_) => panic!("bad params must be BadRequest"),
        }
    }

    #[test]
    fn list_dir_of_a_missing_path_is_not_found() {
        // CPE-1659: a missing path is the ONE domain error a caller most needs to branch on
        // structurally, so it must not be flattened into the generic `Internal` every other domain
        // error gets (see `domain_error_maps_to_internal` below) — proven in-process here, and the same
        // shape the real-server Docker rig's Slice 2 E2E test asserts over an actual wire socket.
        let ctx = HeadlessCtx::new(scratch("base"));
        let resp = Dispatcher::with_builtins()
            .dispatch(&ctx, req("list_dir", json!({ "path": "/definitely/not/a/real/path/xyz" })));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::NotFound, "got {:?}: {}", e.code, e.message),
            Ok(_) => panic!("listing a missing path must error"),
        }
    }

    #[test]
    fn domain_error_maps_to_internal() {
        let ctx = HeadlessCtx::new(scratch("base"));
        // hash_file on a directory errors in the domain, but the directory itself EXISTS — this must
        // stay `Internal`, not `NotFound` (proves `domain_path` doesn't over-fire on every domain error,
        // only a genuinely missing path).
        let d = scratch("hash");
        let resp = Dispatcher::with_builtins().dispatch(&ctx, req("hash_file", json!({ "path": d.to_string_lossy() })));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::Internal),
            Ok(_) => panic!("hashing a folder must error"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    // -----------------------------------------------------------------------------------------
    // CPE-1673 item 4: the missing-path taxonomy used to be a `list_dir`-only one-off. These tests prove
    // `hash_file` and `text_stats` now get the same `NotFound` treatment through the shared
    // `domain_path` helper, and that the classification logic itself never mistakes "can't tell" (a
    // permission-denied traversal) for "isn't there".
    // -----------------------------------------------------------------------------------------

    #[test]
    fn hash_file_of_a_missing_path_is_not_found() {
        let ctx = HeadlessCtx::new(scratch("base"));
        let resp = Dispatcher::with_builtins()
            .dispatch(&ctx, req("hash_file", json!({ "path": "/definitely/not/a/real/path/xyz" })));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::NotFound, "got {:?}: {}", e.code, e.message),
            Ok(_) => panic!("hashing a missing path must error"),
        }
    }

    #[test]
    fn text_stats_of_a_missing_path_is_not_found() {
        let ctx = HeadlessCtx::new(scratch("base"));
        let resp = Dispatcher::with_builtins()
            .dispatch(&ctx, req("text_stats", json!({ "path": "/definitely/not/a/real/path/xyz" })));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::NotFound, "got {:?}: {}", e.code, e.message),
            Ok(_) => panic!("text_stats on a missing path must error"),
        }
    }

    #[test]
    fn classify_path_error_maps_not_found_kind_to_not_found_code() {
        let e = classify_path_error(Some(std::io::ErrorKind::NotFound), "missing".to_string());
        assert_eq!(e.code, ErrorCode::NotFound);
    }

    #[test]
    fn classify_path_error_permission_denied_stays_internal_not_not_found() {
        // The exact regression this item closes: `Path::exists()` collapses ENOENT and EACCES into the
        // same `false`, so a permission-denied parent-directory traversal used to be reported as
        // `NotFound` ("we don't know" reported as "it isn't there"). `PermissionDenied` (or any kind
        // other than `NotFound`) must stay `Internal`.
        let e = classify_path_error(Some(std::io::ErrorKind::PermissionDenied), "denied".to_string());
        assert_eq!(e.code, ErrorCode::Internal);
    }

    #[test]
    fn classify_path_error_existing_path_stays_internal() {
        // No stat error at all (the path exists) — the domain failure is unrelated to existence.
        let e = classify_path_error(None, "not a folder".to_string());
        assert_eq!(e.code, ErrorCode::Internal);
    }

    #[test]
    fn tags_set_then_load_round_trips_through_the_ctx() {
        let base = scratch("tagsbase");
        let ctx = HeadlessCtx::new(&base);
        let d = Dispatcher::with_builtins();
        // set
        let set = d.dispatch(&ctx, req("tags.set", json!({ "path": "/p", "tags": ["a", "b"], "label": "red" })));
        assert!(set.result.is_ok(), "tags.set should succeed: {:?}", set.result);
        // load sees it (same HeadlessCtx config dir)
        let load = d.dispatch(&ctx, req("tags.load", json!({})));
        let store = load.result.expect("tags.load ok");
        assert_eq!(store["/p"]["tags"], json!(["a", "b"]));
        assert_eq!(store["/p"]["label"], json!("red"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn builtins_are_registered() {
        let d = Dispatcher::with_builtins();
        let names = d.methods();
        for m in ["list_dir", "hash_file", "text_stats", "tags.load", "tags.set"] {
            assert!(names.contains(&m), "missing builtin: {m}");
        }
    }
}
