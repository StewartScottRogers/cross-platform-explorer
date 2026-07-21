//! Settings store (CPE-226) — a single on-disk `settings.json` document in the app config dir,
//! from which the frontend hydrates on startup (and defaults to `{}` on a fresh install).
//!
//! Extracted into the Server (CPE-815): the pure `dir`-based helpers were always Tauri-free; the
//! [`load`]/[`save`] entry points take a [`ServerCtx`] to resolve the config dir, so the Tauri
//! commands are one-line dispatchers.

use std::fs;
use std::path::Path;

use crate::ctx::ServerCtx;

/// Read the settings document from `settings.json` in `dir`, returning `{}` when it doesn't exist.
pub fn read_settings_from(dir: &Path) -> String {
    fs::read_to_string(dir.join("settings.json")).unwrap_or_else(|_| "{}".to_string())
}

/// Write the settings document to `settings.json` in `dir`, creating `dir` if needed.
pub fn write_settings_to(dir: &Path, contents: &str) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("settings.json"), contents.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read the on-disk settings document; `{}` on a fresh install so the frontend starts from defaults.
pub fn load(ctx: &dyn ServerCtx) -> Result<String, String> {
    Ok(read_settings_from(&ctx.app_config_dir()?))
}

/// Persist the full settings document, creating the config dir if needed.
pub fn save(ctx: &dyn ServerCtx, contents: &str) -> Result<(), String> {
    write_settings_to(&ctx.app_config_dir()?, contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::HeadlessCtx;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-settings-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn settings_round_trip_and_default_to_empty_object() {
        let d = scratch("settings");
        assert_eq!(read_settings_from(&d), "{}");
        let doc = r#"{"cpe.view":"list","cpe.showHidden":true}"#;
        write_settings_to(&d, doc).unwrap();
        assert_eq!(read_settings_from(&d), doc);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn write_settings_creates_the_config_dir() {
        let d = scratch("settings_mkdir").join("nested/config");
        assert!(!d.exists());
        write_settings_to(&d, "{}").unwrap();
        assert!(d.join("settings.json").exists());
        let _ = fs::remove_dir_all(d.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn ctx_load_save_round_trip() {
        let base = scratch("settings_ctx");
        let ctx = HeadlessCtx::new(&base);
        assert_eq!(load(&ctx).unwrap(), "{}");
        save(&ctx, r#"{"a":1}"#).unwrap();
        assert_eq!(load(&ctx).unwrap(), r#"{"a":1}"#);
        let _ = fs::remove_dir_all(&base);
    }
}
