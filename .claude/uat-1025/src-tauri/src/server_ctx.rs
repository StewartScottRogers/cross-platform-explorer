//! `TauriCtx` — the Tauri-backed implementation of the Server's runtime seam
//! ([`cpe_server::ctx::ServerCtx`], CPE-814/815).
//!
//! The `ServerCtx` trait and its headless implementation now live in the pure `cpe-server`
//! crate; this is the thin adapter the Tauri app provides so command logic can resolve the
//! app-data/config/cache dirs and emit events through the seam instead of reaching for
//! `AppHandle` directly. Owns a cheap `AppHandle` clone, so it is `'static` and can move into
//! spawned tasks / event closures.

use std::path::PathBuf;

use cpe_server::ctx::ServerCtx;

/// The real [`ServerCtx`], backed by a Tauri [`AppHandle`](tauri::AppHandle).
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
