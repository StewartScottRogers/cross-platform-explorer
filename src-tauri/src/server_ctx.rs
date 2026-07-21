//! `ServerCtx` — the runtime seam between the explorer's command logic and Tauri (CPE-814,
//! epic CPE-810).
//!
//! The backend commands historically reach for concrete Tauri types (`AppHandle` to resolve
//! the app-data/config/cache dirs and to `emit` events). That welds the command logic to
//! Tauri and blocks running the Server headless or remote. `ServerCtx` names *exactly* what
//! those commands need from the runtime — directory resolution, event emit, and a
//! cancellation signal — behind a small object-safe trait, so the command logic depends on
//! the trait, not on Tauri.
//!
//! Two implementations:
//! - [`TauriCtx`] — the real one, delegating to a held `AppHandle` (it owns a cheap clone, so
//!   it is `'static` and can move into spawned tasks / event closures).
//! - [`HeadlessCtx`] — a Tauri-free implementation over an explicit base directory that
//!   captures emitted events, so command logic is unit-testable off the runtime and the
//!   Server can later run headless (CPE-815).
//!
//! This is the seam only — extracting the pure `server` crate behind it is CPE-815. Shared
//! runtime *state* (`State<…>`) stays as Tauri dependency-injection for now; that is a
//! separate, legitimate abstraction and is addressed with the crate extraction.

use std::path::PathBuf;

/// What the explorer's command logic needs from the runtime, abstracted off concrete Tauri
/// types. Object-safe so commands can take `&dyn ServerCtx`.
pub trait ServerCtx: Send + Sync {
    /// The per-user application **data** directory (durable state; e.g. the audit journal).
    fn app_data_dir(&self) -> Result<PathBuf, String>;
    /// The per-user application **config** directory (settings, tags).
    fn app_config_dir(&self) -> Result<PathBuf, String>;
    /// The per-user application **cache** directory (regenerable; e.g. thumbnails).
    fn app_cache_dir(&self) -> Result<PathBuf, String>;
    /// Emit an event to the frontend with a JSON payload. Errors are returned rather than
    /// panicking; fire-and-forget callers may ignore them (as the raw `emit` sites did).
    fn emit_json(&self, event: &str, payload: serde_json::Value) -> Result<(), String>;
    /// Whether the current operation has been asked to cancel. Defaults to `false`; the Tauri
    /// runtime does not drive cooperative cancellation through this seam yet (the streaming +
    /// transfer commands use their own generation/`AtomicBool` tokens), but headless callers can,
    /// and the extracted Server routes cancellation here (CPE-815/665 follow-up). Part of the seam
    /// the ticket requires the trait to *cover*, hence retained though not yet called in-tree.
    #[allow(dead_code)]
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// The real [`ServerCtx`], backed by a Tauri [`AppHandle`](tauri::AppHandle). Owns a clone of
/// the handle (cheap — it is reference-counted) so it is `'static` and can be moved into
/// spawned blocking tasks and event closures.
#[derive(Clone)]
pub struct TauriCtx {
    app: tauri::AppHandle,
}

impl TauriCtx {
    /// Wrap an `AppHandle` (cloning it) as a `ServerCtx`.
    pub fn new(app: &tauri::AppHandle) -> Self {
        Self { app: app.clone() }
    }
}

impl ServerCtx for TauriCtx {
    fn app_data_dir(&self) -> Result<PathBuf, String> {
        use tauri::Manager;
        self.app.path().app_data_dir().map_err(|e| e.to_string())
    }

    fn app_config_dir(&self) -> Result<PathBuf, String> {
        use tauri::Manager;
        self.app.path().app_config_dir().map_err(|e| e.to_string())
    }

    fn app_cache_dir(&self) -> Result<PathBuf, String> {
        use tauri::Manager;
        self.app.path().app_cache_dir().map_err(|e| e.to_string())
    }

    fn emit_json(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        use tauri::Emitter;
        self.app.emit(event, payload).map_err(|e| e.to_string())
    }
}

/// A Tauri-free [`ServerCtx`] over an explicit base directory, for headless use and tests.
/// The three dirs are `base/data`, `base/config`, `base/cache`; emitted events are captured
/// in-memory for assertions.
#[cfg(test)]
pub struct HeadlessCtx {
    base: PathBuf,
    emitted: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
    cancelled: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl HeadlessCtx {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self {
            base: base.into(),
            emitted: std::sync::Mutex::new(Vec::new()),
            cancelled: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Snapshot of the events emitted so far, as `(event, payload)` pairs.
    pub fn emitted(&self) -> Vec<(String, serde_json::Value)> {
        self.emitted.lock().unwrap().clone()
    }

    /// Mark the context cancelled (so [`ServerCtx::is_cancelled`] returns `true`).
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
impl ServerCtx for HeadlessCtx {
    fn app_data_dir(&self) -> Result<PathBuf, String> {
        Ok(self.base.join("data"))
    }
    fn app_config_dir(&self) -> Result<PathBuf, String> {
        Ok(self.base.join("config"))
    }
    fn app_cache_dir(&self) -> Result<PathBuf, String> {
        Ok(self.base.join("cache"))
    }
    fn emit_json(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        self.emitted
            .lock()
            .unwrap()
            .push((event.to_string(), payload));
        Ok(())
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_ctx_resolves_dirs_under_base() {
        let ctx = HeadlessCtx::new("/tmp/base");
        assert_eq!(ctx.app_data_dir().unwrap(), PathBuf::from("/tmp/base/data"));
        assert_eq!(
            ctx.app_config_dir().unwrap(),
            PathBuf::from("/tmp/base/config")
        );
        assert_eq!(ctx.app_cache_dir().unwrap(), PathBuf::from("/tmp/base/cache"));
    }

    #[test]
    fn headless_ctx_captures_emits() {
        let ctx = HeadlessCtx::new("/tmp/base");
        let dyn_ctx: &dyn ServerCtx = &ctx;
        dyn_ctx
            .emit_json("transfer://progress", serde_json::json!({ "done": 3 }))
            .unwrap();
        let emitted = ctx.emitted();
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].0, "transfer://progress");
        assert_eq!(emitted[0].1["done"], 3);
    }

    #[test]
    fn headless_ctx_cancellation_flips() {
        let ctx = HeadlessCtx::new("/tmp/base");
        assert!(!ctx.is_cancelled());
        ctx.cancel();
        assert!(ctx.is_cancelled());
    }
}
