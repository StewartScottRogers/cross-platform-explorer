//! Tray quick-access list (CPE-946, epic CPE-713): the pure model behind the system-tray menu's quick
//! folder access — a bounded, ordered list of **pinned** + **recent** entries with add/pin/unpin/remove.
//! Pinned entries persist at the top; recents move-to-front on access and evict the oldest past a cap. No
//! tray or OS code here; the tray renders `items()`.
//!
//! **Persistence (CPE-1272):** the state is serialized to `tray_quick.json` in the app data dir via
//! [`load`]/[`save`], so pins + recents survive a restart. The tray wires the icon/menu; this stays
//! Tauri-free (a [`ServerCtx`] resolves the dir), mirroring [`crate::settings`].

use std::fs;
use std::path::Path;

use crate::ctx::ServerCtx;

/// One quick-access entry (a folder or file the tray offers a one-click jump to).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct QuickEntry {
    pub path: String,
    pub label: String,
    pub pinned: bool,
}

/// The tray's quick-access state: pinned entries (in pin order) followed by recents (most-recent first),
/// with a cap on how many recents are retained.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct QuickAccess {
    pinned: Vec<QuickEntry>,
    recent: Vec<QuickEntry>,
    max_recent: usize,
}

impl QuickAccess {
    pub fn new(max_recent: usize) -> Self {
        Self { pinned: Vec::new(), recent: Vec::new(), max_recent: max_recent.max(1) }
    }

    fn is_pinned(&self, path: &str) -> bool {
        self.pinned.iter().any(|e| e.path == path)
    }

    /// Record a visit: a pinned path is left as-is; otherwise the path is moved to the front of recents
    /// (deduped) and the list is capped, evicting the oldest.
    pub fn touch(&mut self, path: &str, label: &str) {
        if self.is_pinned(path) {
            return;
        }
        self.recent.retain(|e| e.path != path);
        self.recent.insert(0, QuickEntry { path: path.into(), label: label.into(), pinned: false });
        self.recent.truncate(self.max_recent);
    }

    /// Pin a path (added to the end of the pinned list; removed from recents). Idempotent.
    pub fn pin(&mut self, path: &str, label: &str) {
        self.recent.retain(|e| e.path != path);
        if !self.is_pinned(path) {
            self.pinned.push(QuickEntry { path: path.into(), label: label.into(), pinned: true });
        }
    }

    /// Unpin a path — it becomes a most-recent entry again (so it isn't lost).
    pub fn unpin(&mut self, path: &str) {
        if let Some(i) = self.pinned.iter().position(|e| e.path == path) {
            let mut e = self.pinned.remove(i);
            e.pinned = false;
            self.recent.retain(|r| r.path != e.path);
            self.recent.insert(0, e);
            self.recent.truncate(self.max_recent);
        }
    }

    /// Remove a path from wherever it is.
    pub fn remove(&mut self, path: &str) {
        self.pinned.retain(|e| e.path != path);
        self.recent.retain(|e| e.path != path);
    }

    /// The menu, pinned-first then recents (most-recent first).
    pub fn items(&self) -> Vec<&QuickEntry> {
        self.pinned.iter().chain(self.recent.iter()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.pinned.is_empty() && self.recent.is_empty()
    }
}

/// The persisted file name inside the app data dir.
const FILE: &str = "tray_quick.json";

/// Deserialize the state from `dir/tray_quick.json`, falling back to a fresh, empty [`QuickAccess`] with
/// the given cap whenever the file is missing/corrupt (so a bad file never breaks the tray). The cap is
/// re-clamped to at least 1 and the recents re-capped defensively in case an on-disk `max_recent` was 0
/// or the recents grew past the cap in an older format.
pub fn read_from(dir: &Path, max_recent: usize) -> QuickAccess {
    match fs::read_to_string(dir.join(FILE)) {
        Ok(s) => match serde_json::from_str::<QuickAccess>(&s) {
            Ok(mut qa) => {
                qa.max_recent = qa.max_recent.max(1);
                qa.recent.truncate(qa.max_recent);
                qa
            }
            Err(_) => QuickAccess::new(max_recent),
        },
        Err(_) => QuickAccess::new(max_recent),
    }
}

/// Persist the state to `dir/tray_quick.json`, creating `dir` if needed.
pub fn write_to(dir: &Path, qa: &QuickAccess) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(qa).map_err(|e| e.to_string())?;
    fs::write(dir.join(FILE), json.as_bytes()).map_err(|e| e.to_string())
}

/// Load the persisted quick-access state via the [`ServerCtx`] app-data dir; a fresh empty state on any
/// error so the tray always has something to render.
pub fn load(ctx: &dyn ServerCtx, max_recent: usize) -> QuickAccess {
    match ctx.app_data_dir() {
        Ok(dir) => read_from(&dir, max_recent),
        Err(_) => QuickAccess::new(max_recent),
    }
}

/// Save the quick-access state via the [`ServerCtx`] app-data dir.
pub fn save(ctx: &dyn ServerCtx, qa: &QuickAccess) -> Result<(), String> {
    write_to(&ctx.app_data_dir()?, qa)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(q: &QuickAccess) -> Vec<String> {
        q.items().iter().map(|e| e.path.clone()).collect()
    }

    #[test]
    fn touch_moves_to_front_dedups_and_caps() {
        let mut q = QuickAccess::new(2);
        q.touch("/a", "a");
        q.touch("/b", "b");
        q.touch("/a", "a"); // /a back to front
        assert_eq!(paths(&q), vec!["/a", "/b"]);
        q.touch("/c", "c"); // caps at 2 → evict oldest (/b)
        assert_eq!(paths(&q), vec!["/c", "/a"]);
    }

    #[test]
    fn pinned_come_first_and_survive_the_recent_cap() {
        let mut q = QuickAccess::new(2);
        q.pin("/keep", "keep");
        q.touch("/x", "x");
        q.touch("/y", "y");
        q.touch("/z", "z"); // recents capped to [z, y]; pinned unaffected
        assert_eq!(paths(&q), vec!["/keep", "/z", "/y"]);
        // Touching a pinned path is a no-op (doesn't duplicate into recents).
        q.touch("/keep", "keep");
        assert_eq!(paths(&q), vec!["/keep", "/z", "/y"]);
    }

    #[test]
    fn pin_removes_from_recents_unpin_restores_as_recent() {
        let mut q = QuickAccess::new(3);
        q.touch("/a", "a");
        q.pin("/a", "a"); // moves from recent to pinned
        assert_eq!(paths(&q), vec!["/a"]);
        assert!(q.items()[0].pinned);
        q.unpin("/a");
        assert_eq!(paths(&q), vec!["/a"]);
        assert!(!q.items()[0].pinned); // back to a recent
    }

    #[test]
    fn remove_clears_from_both_lists() {
        let mut q = QuickAccess::new(3);
        q.pin("/p", "p");
        q.touch("/r", "r");
        q.remove("/p");
        q.remove("/r");
        assert!(q.is_empty());
    }

    // --- Persistence (CPE-1272) ---------------------------------------------------------------
    use crate::ctx::HeadlessCtx;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-tray-{tag}"))
    }

    #[test]
    fn read_from_defaults_to_empty_when_absent() {
        let d = scratch("absent");
        let q = super::read_from(&d, 5);
        assert!(q.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn write_then_read_round_trips_pins_and_recents() {
        let d = scratch("round");
        let mut q = QuickAccess::new(3);
        q.pin("/keep", "keep");
        q.touch("/a", "a");
        q.touch("/b", "b");
        super::write_to(&d, &q).unwrap();

        let loaded = super::read_from(&d, 3);
        assert_eq!(loaded, q);
        assert_eq!(paths(&loaded), vec!["/keep", "/b", "/a"]);
        assert!(loaded.items()[0].pinned);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn read_from_recovers_from_a_corrupt_file() {
        let d = scratch("corrupt");
        std::fs::write(d.join("tray_quick.json"), b"not json {{{").unwrap();
        let q = super::read_from(&d, 4);
        assert!(q.is_empty()); // corrupt → fresh, never a panic
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ctx_load_save_round_trip_and_recents_update() {
        let base = scratch("ctx");
        let ctx = HeadlessCtx::new(base.to_path_buf());
        // Fresh install → empty.
        assert!(super::load(&ctx, 5).is_empty());

        // Simulate opening two folders (recents update), then persist.
        let mut q = super::load(&ctx, 5);
        q.touch("/proj/one", "one");
        q.touch("/proj/two", "two");
        super::save(&ctx, &q).unwrap();

        // A later process loads exactly what was saved.
        let reloaded = super::load(&ctx, 5);
        assert_eq!(paths(&reloaded), vec!["/proj/two", "/proj/one"]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn read_from_reclamps_a_zero_cap_and_recaps_overlong_recents() {
        let d = scratch("reclamp");
        // Hand-craft a file with max_recent=0 and 3 recents (an older/tampered format).
        let bad = r#"{"pinned":[],"recent":[
            {"path":"/a","label":"a","pinned":false},
            {"path":"/b","label":"b","pinned":false},
            {"path":"/c","label":"c","pinned":false}],"max_recent":0}"#;
        std::fs::write(d.join("tray_quick.json"), bad).unwrap();
        let q = super::read_from(&d, 5);
        // max_recent clamped to >=1, recents truncated to it (defensive).
        assert_eq!(paths(&q), vec!["/a"]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
