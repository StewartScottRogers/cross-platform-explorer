//! Tray quick-access list (CPE-946, epic CPE-713): the pure model behind the system-tray menu's quick
//! folder access — a bounded, ordered list of **pinned** + **recent** entries with add/pin/unpin/remove.
//! Pinned entries persist at the top; recents move-to-front on access and evict the oldest past a cap. No
//! tray or OS code here; the tray renders `items()`.

/// One quick-access entry (a folder or file the tray offers a one-click jump to).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct QuickEntry {
    pub path: String,
    pub label: String,
    pub pinned: bool,
}

/// The tray's quick-access state: pinned entries (in pin order) followed by recents (most-recent first),
/// with a cap on how many recents are retained.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
}
