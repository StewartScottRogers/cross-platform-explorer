//! Server-side contract dispatch (CPE-824, epic CPE-810): turn a [`Request`] envelope into a
//! [`Response`] by looking the method up in a registry and calling the matching `cpe-server` domain
//! function. This is what a network `Client(Rust)` drives over a socket in CPE-820 — here with **no
//! transport**, so it's fully unit-testable. Adding a method = register a handler, no core changes.
//!
//! Error taxonomy at the boundary: an unknown method → [`ErrorCode::NotFound`], params that don't
//! deserialize → [`ErrorCode::BadRequest`], a domain `Err(String)` → [`ErrorCode::Internal`]; a handler
//! never panics the dispatcher.

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
            result(crate::listing::list_dir(&a.path).map_err(domain)?)
        });

        // Path-arg: hash a file.
        d.register("hash_file", |_ctx, p| {
            #[derive(serde::Deserialize)]
            struct P {
                path: String,
            }
            let a: P = params(p)?;
            result(crate::checksum::hash_file(&a.path).map_err(domain)?)
        });

        // Path-arg: text statistics.
        d.register("text_stats", |_ctx, p| {
            #[derive(serde::Deserialize)]
            struct P {
                path: String,
            }
            let a: P = params(p)?;
            result(crate::text_stats::compute(&a.path).map_err(domain)?)
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
    fn domain_error_maps_to_internal() {
        let ctx = HeadlessCtx::new(scratch("base"));
        // hash_file on a directory errors in the domain.
        let d = scratch("hash");
        let resp = Dispatcher::with_builtins().dispatch(&ctx, req("hash_file", json!({ "path": d.to_string_lossy() })));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::Internal),
            Ok(_) => panic!("hashing a folder must error"),
        }
        let _ = std::fs::remove_dir_all(&d);
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
