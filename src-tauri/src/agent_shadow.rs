//! Agent Watch — shadow-content store for write before/after capture (CPE-743, epic CPE-727).
//!
//! To show *what an agent write changed* ("Edit Diff Peek") we need the file content from **before**
//! the change — which the filesystem watcher can't give us, because its `modified` event fires only
//! after the write has landed. This keeps a bounded, in-memory, text-only baseline of files under the
//! watched tree; each mutation is paired against the stored baseline to yield a `{before, after}`
//! record the frontend diffs (via `diff.ts`).
//!
//! **Bounded by design.** Agent Watch's tiebreaker favours visibility over speed/size, but not
//! *unbounded* memory: entries are capped by count and total bytes with LRU eviction, and any single
//! file over a per-file cap (or non-UTF-8 binary, rejected before it reaches here) is not shadowed.
//!
//! **Baseline is lazy, not seeded.** We deliberately do NOT read the whole tree at watch start — on a
//! real repo (`node_modules`, build output) that would be pathological, and a re-read can't recover a
//! true "before" anyway. Instead a baseline is captured on first sighting: a `created` emits with an
//! empty `before` (the whole new file is "added"); a first-seen `modified` records the baseline
//! silently and emits nothing (its diff would be empty); every later `modified` diffs against the
//! stored baseline. Create-then-edit — the common agent pattern — therefore diffs correctly.
//!
//! This module is pure `std` (no tauri, no IO): the watcher reads files and calls in; the store only
//! remembers text and computes pairs, so it is fully unit-testable headlessly.

use std::collections::HashMap;

/// Per-file cap: a file whose UTF-8 text exceeds this is not shadowed (its baseline is dropped).
pub const MAX_FILE_BYTES: usize = 1024 * 1024; // 1 MiB
/// Total cap across all shadowed files; LRU eviction keeps the store under it.
pub const MAX_TOTAL_BYTES: usize = 24 * 1024 * 1024; // 24 MiB
/// Maximum number of shadowed files; LRU eviction keeps the store under it.
pub const MAX_ENTRIES: usize = 4000;

/// A before/after pair for one mutation, ready to emit to the frontend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRecord {
    pub path: String,
    pub before: String,
    pub after: String,
}

struct Entry {
    content: String,
    /// Monotonic touch stamp for LRU eviction (higher = more recently used).
    touched: u64,
}

/// A bounded, in-memory baseline of file text keyed by absolute path. Constructed only while
/// watching (off means off); dropped on stop, which frees everything.
pub struct ShadowStore {
    map: HashMap<String, Entry>,
    total_bytes: usize,
    clock: u64,
    max_file_bytes: usize,
    max_total_bytes: usize,
    max_entries: usize,
}

impl Default for ShadowStore {
    fn default() -> Self {
        Self::with_caps(MAX_FILE_BYTES, MAX_TOTAL_BYTES, MAX_ENTRIES)
    }
}

impl ShadowStore {
    /// A store with the production caps.
    pub fn new() -> Self {
        Self::default()
    }

    /// A store with explicit caps — used by tests to exercise eviction without huge inputs.
    pub fn with_caps(max_file_bytes: usize, max_total_bytes: usize, max_entries: usize) -> Self {
        Self {
            map: HashMap::new(),
            total_bytes: 0,
            clock: 0,
            max_file_bytes,
            max_total_bytes,
            max_entries,
        }
    }

    /// Whether `content` is eligible to be shadowed (within the per-file byte cap). Callers pass only
    /// valid UTF-8 text; binary is rejected by the read step before it gets here.
    pub fn capturable(&self, content: &str) -> bool {
        content.len() <= self.max_file_bytes
    }

    /// Record a `created` file. The baseline is empty, so the record is `(before: "", after: content)`
    /// and `content` becomes the new baseline. Returns `None` (dropping any stale baseline) when the
    /// content is over the per-file cap.
    pub fn on_created(&mut self, path: &str, content: String) -> Option<DiffRecord> {
        if !self.capturable(&content) {
            self.forget(path);
            return None;
        }
        let rec = DiffRecord { path: path.to_owned(), before: String::new(), after: content.clone() };
        self.put(path.to_owned(), content);
        Some(rec)
    }

    /// Record a `modified` file. With a stored baseline, returns `(before: baseline, after: content)`
    /// and updates the baseline. With no baseline (first sighting), stores it silently and returns
    /// `None` — a first-seen modify has no known "before". Over-cap content drops the baseline.
    pub fn on_modified(&mut self, path: &str, content: String) -> Option<DiffRecord> {
        if !self.capturable(&content) {
            self.forget(path);
            return None;
        }
        let before = self.map.get(path).map(|e| e.content.clone());
        self.put(path.to_owned(), content.clone());
        before.map(|before| DiffRecord { path: path.to_owned(), before, after: content })
    }

    /// Drop the baseline for a path (e.g. it was removed/renamed away, or became binary/oversized).
    pub fn forget(&mut self, path: &str) {
        if let Some(e) = self.map.remove(path) {
            self.total_bytes -= e.content.len();
        }
    }

    /// Insert or replace a baseline, keeping `total_bytes` accurate, then evict down to the caps.
    fn put(&mut self, path: String, content: String) {
        let touched = self.tick();
        let new_len = content.len();
        match self.map.get_mut(&path) {
            Some(e) => {
                self.total_bytes -= e.content.len();
                e.content = content;
                e.touched = touched;
                self.total_bytes += new_len;
            }
            None => {
                self.map.insert(path, Entry { content, touched });
                self.total_bytes += new_len;
            }
        }
        self.evict();
    }

    /// Evict the least-recently-touched entries until within both the entry and total-byte caps.
    fn evict(&mut self) {
        while self.map.len() > self.max_entries || self.total_bytes > self.max_total_bytes {
            let victim = self
                .map
                .iter()
                .min_by_key(|(_, e)| e.touched)
                .map(|(k, _)| k.clone());
            match victim {
                Some(k) => self.forget(&k),
                None => break,
            }
        }
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Number of shadowed files. Test/diagnostics only.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether nothing is shadowed. Test/diagnostics only.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Current total shadowed bytes. Test/diagnostics only.
    #[cfg(test)]
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_emits_full_content_as_added() {
        let mut s = ShadowStore::new();
        let rec = s.on_created("/p/a.txt", "hello".into()).expect("created emits");
        assert_eq!(rec.before, "");
        assert_eq!(rec.after, "hello");
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn first_seen_modify_establishes_baseline_silently() {
        let mut s = ShadowStore::new();
        // No baseline yet: records it, emits nothing (before is unknown).
        assert!(s.on_modified("/p/a.txt", "v1".into()).is_none());
        assert_eq!(s.len(), 1);
        // Next modify diffs against the stored baseline.
        let rec = s.on_modified("/p/a.txt", "v2".into()).expect("second modify emits");
        assert_eq!(rec.before, "v1");
        assert_eq!(rec.after, "v2");
    }

    #[test]
    fn created_then_modified_diffs_against_creation() {
        let mut s = ShadowStore::new();
        s.on_created("/p/a.txt", "v1".into()).unwrap();
        let rec = s.on_modified("/p/a.txt", "v2".into()).expect("modify after create emits");
        assert_eq!(rec.before, "v1");
        assert_eq!(rec.after, "v2");
    }

    #[test]
    fn oversized_content_is_skipped_and_drops_baseline() {
        let mut s = ShadowStore::with_caps(4, MAX_TOTAL_BYTES, MAX_ENTRIES);
        // Small first, so a baseline exists.
        s.on_modified("/p/a.txt", "ok".into());
        assert_eq!(s.len(), 1);
        // Now an over-cap write: no record, and the stale baseline is dropped.
        assert!(s.on_modified("/p/a.txt", "toolong".into()).is_none());
        assert_eq!(s.len(), 0);
        assert_eq!(s.total_bytes(), 0);
    }

    #[test]
    fn evicts_least_recently_touched_over_entry_cap() {
        let mut s = ShadowStore::with_caps(MAX_FILE_BYTES, MAX_TOTAL_BYTES, 2);
        s.on_created("/p/a", "a".into());
        s.on_created("/p/b", "b".into());
        // Touch a so b is now the least-recently-used.
        s.on_modified("/p/a", "a2".into());
        s.on_created("/p/c", "c".into()); // over the cap of 2 -> evict LRU (b)
        assert_eq!(s.len(), 2);
        // b evicted: a first-seen modify on it now has no baseline again.
        assert!(s.on_modified("/p/b", "b2".into()).is_none());
    }

    #[test]
    fn evicts_over_total_byte_cap() {
        // Cap total at 5 bytes; entries of 3 bytes each can't both fit.
        let mut s = ShadowStore::with_caps(MAX_FILE_BYTES, 5, MAX_ENTRIES);
        s.on_created("/p/a", "aaa".into());
        s.on_created("/p/b", "bbb".into());
        assert_eq!(s.len(), 1); // a evicted to stay within 5 bytes
        assert!(s.total_bytes() <= 5);
    }

    #[test]
    fn forget_frees_bytes() {
        let mut s = ShadowStore::new();
        s.on_created("/p/a", "hello".into());
        assert_eq!(s.total_bytes(), 5);
        s.forget("/p/a");
        assert!(s.is_empty());
        assert_eq!(s.total_bytes(), 0);
    }
}
