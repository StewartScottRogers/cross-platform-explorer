//! Embedded terminal dock — tab model (CPE-947, epic CPE-714): the pure tab/session bookkeeping behind
//! the terminal panel — a list of terminal tabs (id, title, cwd), a single active tab, and
//! open/close/activate/rename with sane active-tab fixup. No PTY/process here; the dock spawns the real
//! shells and renders these tabs.

/// One terminal tab.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TermTab {
    pub id: u64,
    pub title: String,
    pub cwd: String,
}

/// The dock's tab strip + which tab is active.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TerminalDock {
    tabs: Vec<TermTab>,
    active: usize,
    next_id: u64,
}

impl TerminalDock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a new tab in `cwd` (auto-titled from the cwd's last path segment when `title` is empty),
    /// make it active, and return its id.
    pub fn open(&mut self, cwd: &str, title: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let title = if title.trim().is_empty() { basename(cwd) } else { title.to_string() };
        self.tabs.push(TermTab { id, title, cwd: cwd.to_string() });
        self.active = self.tabs.len() - 1;
        id
    }

    /// Close the tab with `id`. The active tab is kept valid (moves to a neighbour, clamped).
    pub fn close(&mut self, id: u64) {
        if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
            self.tabs.remove(i);
            if self.tabs.is_empty() {
                self.active = 0;
            } else if i < self.active || self.active >= self.tabs.len() {
                self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
            }
        }
    }

    /// Make the tab with `id` active (no-op if unknown).
    pub fn activate(&mut self, id: u64) {
        if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
            self.active = i;
        }
    }

    /// Rename a tab (no-op if unknown or blank).
    pub fn rename(&mut self, id: u64, title: &str) {
        if !title.trim().is_empty() {
            if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
                t.title = title.to_string();
            }
        }
    }

    /// Update a tab's working directory (e.g. after a `cd`); retitles it if the title was still the
    /// auto-title of the old cwd.
    pub fn set_cwd(&mut self, id: u64, cwd: &str) {
        if let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) {
            if t.title == basename(&t.cwd) {
                t.title = basename(cwd);
            }
            t.cwd = cwd.to_string();
        }
    }

    pub fn active_tab(&self) -> Option<&TermTab> {
        self.tabs.get(self.active)
    }
    pub fn tabs(&self) -> &[TermTab] {
        &self.tabs
    }
    pub fn len(&self) -> usize {
        self.tabs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }
}

/// Managed-state wrapper around a single [`TerminalDock`] (CPE-1242, epic CPE-714): the Tauri app holds
/// one of these in managed state so the `terminal_dock_*` commands are thin dispatchers into it,
/// mirroring [`crate::index_service::IndexService`]. Cheaply cloneable (an `Arc` around the mutex) and
/// zero-cost until a tab is opened — no docked panel means an empty dock and no PTY (the PTY session
/// registry that actually spawns shells lives separately, in the app's `pty` module).
#[derive(Clone, Default)]
pub struct TerminalDockState(std::sync::Arc<std::sync::Mutex<TerminalDock>>);

impl TerminalDockState {
    pub fn open(&self, cwd: &str, title: &str) -> u64 {
        self.0.lock().unwrap().open(cwd, title)
    }

    pub fn close(&self, id: u64) {
        self.0.lock().unwrap().close(id);
    }

    pub fn activate(&self, id: u64) {
        self.0.lock().unwrap().activate(id);
    }

    pub fn rename(&self, id: u64, title: &str) {
        self.0.lock().unwrap().rename(id, title);
    }

    pub fn set_cwd(&self, id: u64, cwd: &str) {
        self.0.lock().unwrap().set_cwd(id, cwd);
    }

    pub fn tabs(&self) -> Vec<TermTab> {
        self.0.lock().unwrap().tabs().to_vec()
    }

    pub fn active_tab(&self) -> Option<TermTab> {
        self.0.lock().unwrap().active_tab().cloned()
    }
}

fn basename(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let name = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
    if name.is_empty() { path.to_string() } else { name.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_auto_titles_and_activates() {
        let mut d = TerminalDock::new();
        let a = d.open("/home/me/proj", "");
        assert_eq!(d.active_tab().unwrap().title, "proj"); // auto-titled from basename
        assert_eq!(d.active_tab().unwrap().id, a);
        let b = d.open("/tmp", "scratch");
        assert_eq!(d.active_tab().unwrap().id, b); // newest is active
        assert_eq!(d.active_tab().unwrap().title, "scratch");
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn closing_keeps_a_valid_active_tab() {
        let mut d = TerminalDock::new();
        let a = d.open("/a", "a");
        let b = d.open("/b", "b");
        let c = d.open("/c", "c");
        d.activate(b);
        d.close(b); // active was middle → shifts to a neighbour, still valid
        assert!(d.active_tab().is_some());
        d.close(c);
        d.close(a);
        assert!(d.is_empty());
        assert!(d.active_tab().is_none());
    }

    #[test]
    fn closing_a_tab_before_active_keeps_the_same_active_tab() {
        let mut d = TerminalDock::new();
        let a = d.open("/a", "a");
        let _b = d.open("/b", "b");
        d.activate(_b); // active = index 1 (b)
        d.close(a); // remove index 0 → active should follow b, now at index 0
        assert_eq!(d.active_tab().unwrap().title, "b");
    }

    #[test]
    fn rename_and_set_cwd_retitle_only_when_auto() {
        let mut d = TerminalDock::new();
        let a = d.open("/old", ""); // auto-title "old"
        d.set_cwd(a, "/new"); // still auto → retitles to "new"
        assert_eq!(d.active_tab().unwrap().title, "new");
        d.rename(a, "custom");
        d.set_cwd(a, "/other"); // title is custom now → NOT retitled
        assert_eq!(d.active_tab().unwrap().title, "custom");
        assert_eq!(d.active_tab().unwrap().cwd, "/other");
    }

    #[test]
    fn dock_state_dispatches_to_the_underlying_dock() {
        let state = TerminalDockState::default();
        assert!(state.tabs().is_empty());
        assert!(state.active_tab().is_none());

        let a = state.open("/a", "");
        assert_eq!(state.tabs().len(), 1);
        assert_eq!(state.active_tab().unwrap().id, a);
        assert_eq!(state.active_tab().unwrap().title, "a");

        let b = state.open("/b", "custom");
        assert_eq!(state.active_tab().unwrap().id, b);
        state.activate(a);
        assert_eq!(state.active_tab().unwrap().id, a);
        state.rename(a, "renamed");
        assert_eq!(state.active_tab().unwrap().title, "renamed");
        state.set_cwd(a, "/moved");
        assert_eq!(state.active_tab().unwrap().cwd, "/moved");

        state.close(a);
        state.close(b);
        assert!(state.tabs().is_empty());
    }

    /// A cloned handle shares the same underlying dock (it's an `Arc`), the way the Tauri app clones it
    /// out of managed state into command handlers — a change through one clone is visible through another.
    #[test]
    fn dock_state_clones_share_the_same_dock() {
        let state = TerminalDockState::default();
        let clone = state.clone();
        let id = state.open("/shared", "");
        assert_eq!(clone.tabs().len(), 1);
        assert_eq!(clone.active_tab().unwrap().id, id);
    }
}
