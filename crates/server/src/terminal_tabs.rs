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
}
