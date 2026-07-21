//! `ServerCtx` — the runtime seam between the Server's domain logic and its host (CPE-814,
//! epic CPE-810).
//!
//! Domain logic names *exactly* what it needs from the runtime — app-data/config/cache
//! directory resolution, event emit, and a cancellation signal — behind this small
//! object-safe trait, so it depends on the trait, not on Tauri. The Tauri app supplies the
//! real implementation (`TauriCtx`, in the app crate); [`HeadlessCtx`] is a Tauri-free
//! implementation for headless use and tests.

use std::path::PathBuf;

/// What the Server's domain logic needs from the runtime, abstracted off any concrete host.
/// Object-safe so callers can take `&dyn ServerCtx`.
pub trait ServerCtx: Send + Sync {
    /// The per-user application **data** directory (durable state; e.g. the audit journal).
    fn app_data_dir(&self) -> Result<PathBuf, String>;
    /// The per-user application **config** directory (settings, tags).
    fn app_config_dir(&self) -> Result<PathBuf, String>;
    /// The per-user application **cache** directory (regenerable; e.g. thumbnails).
    fn app_cache_dir(&self) -> Result<PathBuf, String>;
    /// Emit an event to the frontend with a JSON payload. Errors are returned rather than
    /// panicking; fire-and-forget callers may ignore them.
    fn emit_json(&self, event: &str, payload: serde_json::Value) -> Result<(), String>;
    /// Whether the current operation has been asked to cancel. Defaults to `false`.
    #[allow(dead_code)]
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// A Tauri-free [`ServerCtx`] over an explicit base directory, for headless use and tests.
/// The three dirs are `base/data`, `base/config`, `base/cache`; emitted events are captured
/// in-memory for assertions.
pub struct HeadlessCtx {
    base: PathBuf,
    emitted: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
    cancelled: std::sync::atomic::AtomicBool,
}

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
