//! Server-side contract dispatch (CPE-824, epic CPE-810): turn a [`Request`] envelope into a
//! [`Response`] by looking the method up in a registry and calling the matching `cpe-server` domain
//! function. This is what a network `Client(Rust)` drives over a socket in CPE-820 — here with **no
//! transport**, so it's fully unit-testable. Adding a method = register a handler, no core changes.
//!
//! Error taxonomy at the boundary: an unknown method â†’ [`ErrorCode::NotFound`], params that don't
//! deserialize â†’ [`ErrorCode::BadRequest`]; a domain `Err(String)` from a **path-taking** handler goes
//! through [`domain_path`], which is `NotFound` when the path genuinely doesn't exist and `Internal`
//! otherwise (including when the path's existence can't even be determined, e.g. a permission-denied
//! parent-directory traversal — "we don't know" must never be reported as "it isn't there"); every other
//! domain `Err(String)` â†’ [`ErrorCode::Internal`] via [`domain`]. A handler never panics the dispatcher.

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

// CPE-1692 decide-and-log: the crate also has ~20 sites of the shape `if !root_path.is_dir() { Err("…
// not a folder") }` (checksum.rs, compare.rs, content_index.rs, content_search.rs,
// document_similarity.rs, dangling_links_scan.rs's own root check, duplicates.rs, disk_usage.rs's other
// two sites, folder_similarity_scan.rs, folder_template.rs, folder_stats.rs, image_similarity.rs,
// index.rs, links.rs's `create_junction`, name_search.rs, snapshot_capture.rs, vault_manager.rs, …).
// Those collapse through the identical `false`-swallows-every-`stat`-failure mechanism this ticket
// fixes, so a permission-denied root gets the same "not a folder" a genuinely-wrong-type root does —
// which is a real inaccuracy, but a materially SMALLER lie than this ticket's sites: it doesn't claim
// the path is *absent* (the specific "we don't know" -> "it isn't there" failure `classify_path_error`
// exists to prevent), only that its *type* is wrong, and the batch operation it gates still refuses to
// proceed either way. Left unfixed here deliberately, not overlooked: retrofitting `metadata()` +
// classification onto ~20 sites each with its own message contract is a second ticket's worth of sweep
// and per-site test work, and mixing it into this one is exactly how CPE-1678/1687's sweeps under-covered
// their own conclusions (Ticketing/wiki.md Evidence Rules) — bundling scope in has repeatedly cost more
// coverage than it bought. Recommend a follow-up ticket scoped to that family alone.

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

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-dispatch-{tag}"))
    }

    fn req(method: &str, params: serde_json::Value) -> Request {
        Request { method: method.to_string(), params }
    }

    #[test]
    fn dispatch_list_dir_returns_entries() {
        let d = scratch("list");
        std::fs::write(d.join("a.txt"), b"hi").unwrap();
        let ctx_base = scratch("base");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
        let resp = Dispatcher::with_builtins().dispatch(&ctx, req("list_dir", json!({ "path": d.to_string_lossy() })));
        let val = resp.result.expect("list_dir should succeed");
        let names: Vec<String> = val.as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_string()).collect();
        assert!(names.iter().any(|n| n == "a.txt"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn unknown_method_is_not_found() {
        let ctx_base = scratch("base");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
        let resp = Dispatcher::with_builtins().dispatch(&ctx, req("does_not_exist", json!({})));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::NotFound),
            Ok(_) => panic!("unknown method must be NotFound"),
        }
    }

    #[test]
    fn bad_params_are_a_bad_request() {
        let ctx_base = scratch("base");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
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
        let ctx_base = scratch("base");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
        let resp = Dispatcher::with_builtins()
            .dispatch(&ctx, req("list_dir", json!({ "path": "/definitely/not/a/real/path/xyz" })));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::NotFound, "got {:?}: {}", e.code, e.message),
            Ok(_) => panic!("listing a missing path must error"),
        }
    }

    #[test]
    fn domain_error_maps_to_internal() {
        let ctx_base = scratch("base");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
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
        let ctx_base = scratch("base");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
        let resp = Dispatcher::with_builtins()
            .dispatch(&ctx, req("hash_file", json!({ "path": "/definitely/not/a/real/path/xyz" })));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::NotFound, "got {:?}: {}", e.code, e.message),
            Ok(_) => panic!("hashing a missing path must error"),
        }
    }

    #[test]
    fn text_stats_of_a_missing_path_is_not_found() {
        let ctx_base = scratch("base");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
        let resp = Dispatcher::with_builtins()
            .dispatch(&ctx, req("text_stats", json!({ "path": "/definitely/not/a/real/path/xyz" })));
        match resp.result {
            Err(e) => assert_eq!(e.code, ErrorCode::NotFound, "got {:?}: {}", e.code, e.message),
            Ok(_) => panic!("text_stats on a missing path must error"),
        }
    }

    /// Try to make `path` readable-to-`stat` but unreadable-to-`read`, and report whether the denial
    /// actually took. Used by [`text_stats_separates_a_read_failure_from_a_not_text_verdict`].
    ///
    /// Deliberately conservative: it returns `false` unless the resulting file is *both* unreadable
    /// (so the read-failure branch is genuinely exercised) *and* still stattable (so `compute` reaches
    /// that branch instead of failing earlier at `metadata`). Running elevated / as root, or on a
    /// filesystem that ignores ACLs and mode bits, leaves the file readable — the caller then skips
    /// rather than fails, because a machine that cannot construct a denied path is not evidence of a
    /// bug. CI runs Linux + macOS + Windows, so both branches are exercised somewhere.
    ///
    /// `#[track_caller]` for the same reason the three `fsutil` staging helpers carry it (CPE-1717):
    /// when `require_staged` below turns a failed staging into a panic, the report must name the leg
    /// that called this, not this function. Harmless today with a single caller in this file — and
    /// wrong the moment a second one appears, which is exactly when nobody re-reads it.
    #[track_caller]
    fn deny_read(path: &std::path::Path) -> bool {
        #[cfg(windows)]
        {
            // `(RD)` denies FILE_READ_DATA *only*, leaving READ_ATTRIBUTES intact — a broader
            // `(R)`/`(F)` deny would also break `fs::metadata` and short-circuit the test above the
            // code it is meant to cover.
            let Ok(user) = std::env::var("USERNAME") else { return false };
            if user.is_empty() {
                return false;
            }
            let _ = std::process::Command::new("icacls")
                .arg(path)
                .arg("/deny")
                .arg(format!("{user}:(RD)"))
                .output();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000));
        }
        // CPE-1717: `supported_here = true` — `(RD)` on Windows and `chmod 0o000` on Unix both leave
        // the entry stattable while refusing its bytes, on every platform CI runs. So a failure to
        // stage is a runner that changed under us, and under CI that is red rather than a notice
        // printed inside a passing run of a 2,100-test suite. See `fsutil::require_staged`.
        crate::fsutil::require_staged(
            "deny_read",
            true,
            std::fs::read(path).is_err() && std::fs::metadata(path).is_ok(),
        )
    }

    /// Undo [`deny_read`] so the scratch directory can be removed.
    fn allow_read(path: &std::path::Path) {
        #[cfg(windows)]
        {
            if let Ok(user) = std::env::var("USERNAME") {
                let _ = std::process::Command::new("icacls")
                    .arg(path)
                    .arg("/remove:d")
                    .arg(&user)
                    .output();
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
    }

    #[test]
    fn text_stats_separates_a_read_failure_from_a_not_text_verdict() {
        // CPE-1678. The UAT on PR #860 found `text_stats` answering a *read* failure with a *content*
        // verdict: a permission-denied path came back `Internal` — the right code — with the message
        // "not a text file", sending the user to inspect bytes nobody had managed to read. All three
        // outcomes are asserted here through the real `Dispatcher`, not the `compute` helper, because
        // the message is what reaches a caller and the helper's return type isn't.
        let ctx_base = scratch("base");
        let ctx = HeadlessCtx::new(ctx_base.to_path_buf());
        let d = Dispatcher::with_builtins();
        let dir = scratch("textstats");

        // 1. Missing path -> NotFound (the CPE-1673 taxonomy, unchanged).
        let missing = d.dispatch(&ctx, req("text_stats", json!({ "path": "/definitely/not/a/real/path/xyz" })));
        match missing.result {
            Err(e) => assert_eq!(e.code, ErrorCode::NotFound, "got {:?}: {}", e.code, e.message),
            Ok(_) => panic!("text_stats on a missing path must error"),
        }

        // 2. A file that was read fine and simply isn't UTF-8 -> the honest content verdict.
        let bin = dir.join("bin");
        std::fs::write(&bin, [0xff, 0xfe, 0x00]).unwrap();
        let not_text = d.dispatch(&ctx, req("text_stats", json!({ "path": bin.to_string_lossy() })));
        match not_text.result {
            Err(e) => {
                assert_eq!(e.code, ErrorCode::Internal, "got {:?}: {}", e.code, e.message);
                assert!(e.message.contains("not a text file"), "got {}", e.message);
            }
            Ok(_) => panic!("text_stats on a binary file must error"),
        }

        // 3. A file that could not be read at all -> still `Internal`, but naming the access failure
        //    instead of pronouncing on contents nobody read.
        let denied = dir.join("denied.txt");
        std::fs::write(&denied, b"readable text\n").unwrap();

        // `allow_read` must run even when an assertion below panics, or a red run leaves a
        // permanently unreadable file behind in the temp dir — and this repo *mandates* a red run
        // for every guard, so the leak would be once per ticket per developer machine, not a rare
        // event. The reviewer of PR #865 found three such orphans, two from this PR's own
        // red/green cycle. A `Drop` guard is the only thing that survives an unwind, and it has to
        // be armed **before** the assertions: a plain call after them never runs on the one path
        // that actually leaks.
        struct Restore<'a>(&'a std::path::Path, &'a std::path::Path);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                allow_read(self.0);
                let _ = std::fs::remove_dir_all(self.1);
            }
        }
        let _restore = Restore(&denied, &dir);

        if deny_read(&denied) {
            let resp = d.dispatch(&ctx, req("text_stats", json!({ "path": denied.to_string_lossy() })));
            match resp.result {
                Err(e) => {
                    assert_eq!(e.code, ErrorCode::Internal, "got {:?}: {}", e.code, e.message);
                    assert!(
                        !e.message.contains("not a text file"),
                        "a read failure must not be reported as a content verdict: {}",
                        e.message
                    );
                    // The OS's own words: "Access is denied. (os error 5)" / "Permission denied (os
                    // error 13)". Matching on "denied" keeps this true on every platform in the matrix.
                    assert!(
                        e.message.to_lowercase().contains("denied"),
                        "the message must name the real cause: {}",
                        e.message
                    );
                }
                Ok(_) => panic!("text_stats on an unreadable file must error"),
            }
        } else {
            // A machine that cannot construct a denied path (elevated/root, or an ACL-less
            // filesystem) is not evidence of a bug, so this leg skips rather than fails. But it
            // **says so**, because leg 3 is the only leg that tests CPE-1678 at all — legs 1 and 2
            // assert behaviour that predates this fix. If the denial ever silently stops working
            // (CI moves to a root container, a runner image changes `icacls`, a filesystem stops
            // honouring mode bits), a silent skip would leave this test passing while guarding
            // nothing, and every board would stay green. Reviewer of PR #865 proved the failure
            // mode by patching `deny_read` to return `false`: the run was byte-identical to a real
            // one. Announcing the skip is the difference between "verified" and "did not check".
            //
            // Deliberately not a hard failure: see CPE-1680, which is about exactly this shape —
            // an unknown quietly folded into the one bucket that means "safe to ignore". The rule
            // there is that "I don't know" must red the run **or be reported loudly**; this is the
            // second.
            //
            // `writeln!(std::io::stderr(), ..)` and NOT `eprintln!` — this is load-bearing, so do not
            // "simplify" it back. libtest captures stdout/stderr per test and replays it only for
            // FAILING tests; a skip is a pass, so an `eprintln!` here is swallowed and never reaches
            // the log. The capture works by intercepting the `print!`/`eprint!` macros, so writing to
            // the process's stderr handle directly goes around it. CI runs plain `cargo test` with no
            // `--nocapture` (`.github/workflows/ci.yml`), which is exactly the case that matters:
            // a message only a developer sees when they remember a flag is not a report.
            //
            // This was got wrong once already, in this very block, by a fix whose comment asserted
            // "the CI log shows it unconditionally" without checking — a confident claim about an
            // unverified mechanism, inside a test about confident claims standing in for unknowns.
            // Caught by the PR #865 reviewer, who forced the skip without `--nocapture` and got a run
            // byte-identical to a real one. The lesson generalises: verify through the channel that
            // will actually carry the message.
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1678] SKIPPED the read-failure leg: could not make {} unreadable-but-stattable \
                 on this machine (elevated/root, or a filesystem ignoring ACLs and mode bits). \
                 The remaining assertions do NOT cover CPE-1678.",
                denied.display()
            );
        }
        // `_restore` cleans up on the way out, panic or not.
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
        let ctx = HeadlessCtx::new(base.to_path_buf());
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
