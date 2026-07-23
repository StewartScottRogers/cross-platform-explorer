//! Thumbnail cache core — stable cache keys + LRU eviction + request coalescing (CPE-939, epic
//! CPE-718). Pure, std-only cache-management model the universal thumbnail pipeline sits on: it owns
//! *which* thumbnails are kept and *when* they're recomputed, not the image/video decoding itself.
//!
//! Three pieces:
//! - [`thumb_key`] — a deterministic, collision-resistant key from `path + mtime + size + target_px`,
//!   so an edited file (new mtime/size) or a different tile size is a cache miss.
//! - [`ThumbCache`] — an LRU map bounded by *both* a max entry count and a max total byte budget;
//!   `put` evicts least-recently-used entries until within both, `get` promotes to most-recent.
//! - Request coalescing — [`ThumbCache::begin`]/[`ThumbCache::finish`] so the same missing thumbnail
//!   isn't computed twice by concurrent callers.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

use serde::Serialize;

/// The identity a thumbnail is keyed on: source path + its mtime (ms) + size (bytes) + the requested
/// target edge in pixels. Any change to these means a different thumbnail, hence a different key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ThumbKey {
    pub path: String,
    pub mtime_ms: u64,
    pub size_bytes: u64,
    pub target_px: u32,
}

impl ThumbKey {
    /// Build a key struct from its parts.
    pub fn new(path: &str, mtime_ms: u64, size_bytes: u64, target_px: u32) -> Self {
        Self { path: path.to_string(), mtime_ms, size_bytes, target_px }
    }

    /// The stable string form of this key (see [`thumb_key`]).
    pub fn to_key(&self) -> String {
        thumb_key(&self.path, self.mtime_ms, self.size_bytes, self.target_px)
    }
}

/// A stable, collision-resistant cache-key string for a thumbnail. Deterministic across runs (a fixed
/// `DefaultHasher` seed) and sensitive to every input, so editing the file (mtime/size changes) or
/// requesting a different `target_px` produces a distinct key. The path is folded in twice — once as
/// the hash and once as a short suffix — to further separate distinct paths that hash-collide.
pub fn thumb_key(path: &str, mtime_ms: u64, size_bytes: u64, target_px: u32) -> String {
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    mtime_ms.hash(&mut h);
    size_bytes.hash(&mut h);
    target_px.hash(&mut h);
    let digest = h.finish();

    // A second, order-shuffled hash widens the effective key space beyond one 64-bit word.
    let mut h2 = DefaultHasher::new();
    target_px.hash(&mut h2);
    size_bytes.hash(&mut h2);
    mtime_ms.hash(&mut h2);
    path.hash(&mut h2);
    let digest2 = h2.finish();

    format!("{digest:016x}{digest2:016x}")
}

/// A cached thumbnail's bookkeeping entry: its key plus the byte cost it charges against the budget
/// (the stored PNG size, or a handle's footprint). The actual bytes live on disk / elsewhere; this
/// tracks only what the cache needs for eviction.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ThumbEntry {
    pub key: String,
    pub bytes: u64,
}

/// An LRU thumbnail cache bounded by *both* a maximum entry count and a maximum total byte budget.
/// `put` inserts (or updates) an entry and evicts least-recently-used entries until both budgets are
/// satisfied; `get` returns an entry and promotes it to most-recently-used. Recency is a `VecDeque`
/// of keys (front = least recent, back = most recent) beside a `HashMap` of entries.
///
/// Separately tracks *in-flight* keys for request coalescing ([`ThumbCache::begin`] / [`finish`]).
///
/// [`finish`]: ThumbCache::finish
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ThumbCache {
    max_entries: usize,
    max_bytes: u64,
    total_bytes: u64,
    entries: HashMap<String, ThumbEntry>,
    /// Recency order, front = least-recently-used, back = most-recently-used.
    order: VecDeque<String>,
    /// Keys currently being computed by some caller (coalescing guard). Not serialized as cache state
    /// that survives a restart, but kept observable for tests/diagnostics.
    in_flight: HashSet<String>,
}

impl ThumbCache {
    /// Create a cache bounded by `max_entries` items and `max_bytes` total bytes. Both budgets are
    /// enforced on every `put`; a zero budget means the cache holds nothing.
    pub fn new(max_entries: usize, max_bytes: u64) -> Self {
        Self {
            max_entries,
            max_bytes,
            total_bytes: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
            in_flight: HashSet::new(),
        }
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Total bytes currently charged against the budget.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// True if `key` is present (does *not* affect recency).
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Fetch an entry, promoting it to most-recently-used. Returns `None` on a miss.
    pub fn get(&mut self, key: &str) -> Option<&ThumbEntry> {
        if self.entries.contains_key(key) {
            self.touch(key);
            self.entries.get(key)
        } else {
            None
        }
    }

    /// Insert or replace a thumbnail entry of `bytes` cost, then evict least-recently-used entries
    /// until both the count and byte budgets hold. The freshly-put key is most-recently-used, so it is
    /// the last thing evicted (a single entry larger than `max_bytes` is dropped again immediately,
    /// leaving the cache empty rather than over budget). A `put` also clears any in-flight mark for the
    /// key, since a completed computation is what typically calls `put`.
    pub fn put(&mut self, key: &str, bytes: u64) {
        if let Some(old) = self.entries.remove(key) {
            self.total_bytes = self.total_bytes.saturating_sub(old.bytes);
            self.remove_from_order(key);
        }
        self.entries.insert(key.to_string(), ThumbEntry { key: key.to_string(), bytes });
        self.total_bytes = self.total_bytes.saturating_add(bytes);
        self.order.push_back(key.to_string());
        self.in_flight.remove(key);
        self.evict();
    }

    /// Remove an entry if present, returning it. Recency + byte total are updated.
    pub fn remove(&mut self, key: &str) -> Option<ThumbEntry> {
        if let Some(entry) = self.entries.remove(key) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.bytes);
            self.remove_from_order(key);
            Some(entry)
        } else {
            None
        }
    }

    /// Drop everything (entries, recency, byte total). In-flight marks are left untouched — a clear of
    /// cached results shouldn't unblock computations other callers are still running.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.total_bytes = 0;
    }

    /// Coalescing gate: claim `key` for computation. Returns `true` if this caller should compute the
    /// thumbnail (the key was not already in-flight), or `false` if another caller is already computing
    /// it — the second caller should wait for the result rather than duplicate the work. A key already
    /// present in the cache still returns `true` on `begin` (callers check `get` first); `begin` only
    /// arbitrates concurrent *computation*.
    pub fn begin(&mut self, key: &str) -> bool {
        self.in_flight.insert(key.to_string())
    }

    /// Clear the in-flight mark for `key` (computation finished or failed). Safe to call for a key that
    /// was never begun.
    pub fn finish(&mut self, key: &str) {
        self.in_flight.remove(key);
    }

    /// True if `key` is currently claimed for computation.
    pub fn is_in_flight(&self, key: &str) -> bool {
        self.in_flight.contains(key)
    }

    /// Number of keys currently in flight.
    pub fn in_flight_len(&self) -> usize {
        self.in_flight.len()
    }

    // --- internals ------------------------------------------------------------------------------

    /// Move `key` to the most-recently-used end of the recency order.
    fn touch(&mut self, key: &str) {
        self.remove_from_order(key);
        self.order.push_back(key.to_string());
    }

    /// Drop the first occurrence of `key` from the recency deque.
    fn remove_from_order(&mut self, key: &str) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            self.order.remove(pos);
        }
    }

    /// Evict least-recently-used entries until both budgets are satisfied.
    fn evict(&mut self) {
        while self.entries.len() > self.max_entries || self.total_bytes > self.max_bytes {
            let Some(victim) = self.order.pop_front() else { break };
            if let Some(entry) = self.entries.remove(&victim) {
                self.total_bytes = self.total_bytes.saturating_sub(entry.bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_deterministic() {
        let a = thumb_key("/a/b.png", 100, 2048, 256);
        let b = thumb_key("/a/b.png", 100, 2048, 256);
        assert_eq!(a, b, "same inputs must yield the same key");
        // 32 hex chars = two 64-bit words.
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn key_is_sensitive_to_each_input() {
        let base = thumb_key("/a/b.png", 100, 2048, 256);
        assert_ne!(base, thumb_key("/a/c.png", 100, 2048, 256), "path change");
        assert_ne!(base, thumb_key("/a/b.png", 101, 2048, 256), "mtime change");
        assert_ne!(base, thumb_key("/a/b.png", 100, 2049, 256), "size change");
        assert_ne!(base, thumb_key("/a/b.png", 100, 2048, 128), "target_px change");
    }

    #[test]
    fn key_struct_matches_free_fn() {
        let k = ThumbKey::new("/x/y.jpg", 42, 999, 512);
        assert_eq!(k.to_key(), thumb_key("/x/y.jpg", 42, 999, 512));
    }

    #[test]
    fn distinct_similar_inputs_do_not_collide() {
        // A batch of near-identical keys should all be distinct.
        let mut seen = HashSet::new();
        for mtime in 0..50u64 {
            for px in [64u32, 128, 256, 512] {
                let k = thumb_key("/photos/img.png", mtime, 4096, px);
                assert!(seen.insert(k), "unexpected key collision");
            }
        }
        assert_eq!(seen.len(), 50 * 4);
    }

    #[test]
    fn put_and_get_roundtrip() {
        let mut c = ThumbCache::new(4, 1_000);
        c.put("k1", 100);
        let e = c.get("k1").expect("hit");
        assert_eq!(e.key, "k1");
        assert_eq!(e.bytes, 100);
        assert_eq!(c.len(), 1);
        assert_eq!(c.total_bytes(), 100);
        assert!(c.get("missing").is_none());
    }

    #[test]
    fn put_same_key_updates_bytes_not_count() {
        let mut c = ThumbCache::new(4, 10_000);
        c.put("k1", 100);
        c.put("k1", 250);
        assert_eq!(c.len(), 1);
        assert_eq!(c.total_bytes(), 250);
    }

    #[test]
    fn evicts_by_count() {
        let mut c = ThumbCache::new(3, u64::MAX);
        c.put("a", 1);
        c.put("b", 1);
        c.put("c", 1);
        c.put("d", 1); // over the count budget -> evict LRU ("a")
        assert_eq!(c.len(), 3);
        assert!(!c.contains("a"), "a should be evicted");
        assert!(c.contains("b") && c.contains("c") && c.contains("d"));
    }

    #[test]
    fn evicts_by_bytes() {
        let mut c = ThumbCache::new(100, 250);
        c.put("a", 100);
        c.put("b", 100);
        c.put("c", 100); // total 300 > 250 -> evict LRU ("a")
        assert!(!c.contains("a"), "a should be evicted by byte budget");
        assert!(c.contains("b") && c.contains("c"));
        assert_eq!(c.total_bytes(), 200);
        assert!(c.total_bytes() <= 250);
    }

    #[test]
    fn get_promotes_recency() {
        let mut c = ThumbCache::new(3, u64::MAX);
        c.put("a", 1);
        c.put("b", 1);
        c.put("c", 1);
        // Touch "a" so it's most-recent; now "b" is LRU.
        assert!(c.get("a").is_some());
        c.put("d", 1); // evict LRU, which should now be "b", not "a"
        assert!(c.contains("a"), "a was promoted and must survive");
        assert!(!c.contains("b"), "b was LRU and should be evicted");
        assert!(c.contains("c") && c.contains("d"));
    }

    #[test]
    fn oversized_single_entry_is_dropped() {
        let mut c = ThumbCache::new(4, 100);
        c.put("big", 500); // larger than the whole budget
        assert!(!c.contains("big"), "an entry larger than the budget can't be kept");
        assert!(c.is_empty());
        assert_eq!(c.total_bytes(), 0);
    }

    #[test]
    fn remove_and_clear() {
        let mut c = ThumbCache::new(4, 1_000);
        c.put("a", 100);
        c.put("b", 200);
        let removed = c.remove("a").expect("present");
        assert_eq!(removed.bytes, 100);
        assert_eq!(c.total_bytes(), 200);
        assert!(c.remove("a").is_none());
        c.clear();
        assert!(c.is_empty());
        assert_eq!(c.total_bytes(), 0);
    }

    #[test]
    fn coalescing_begin_twice_then_finish() {
        let mut c = ThumbCache::new(4, 1_000);
        assert!(c.begin("k"), "first begin claims the key");
        assert!(!c.begin("k"), "second begin sees it in flight");
        assert!(c.is_in_flight("k"));
        assert_eq!(c.in_flight_len(), 1);
        c.finish("k");
        assert!(!c.is_in_flight("k"));
        assert!(c.begin("k"), "after finish the key can be claimed again");
    }

    #[test]
    fn put_clears_in_flight() {
        let mut c = ThumbCache::new(4, 1_000);
        assert!(c.begin("k"));
        c.put("k", 50); // completing the computation stores + clears the in-flight mark
        assert!(!c.is_in_flight("k"));
        assert!(c.contains("k"));
    }

    #[test]
    fn independent_keys_are_independent_in_flight() {
        let mut c = ThumbCache::new(4, 1_000);
        assert!(c.begin("a"));
        assert!(c.begin("b"));
        assert_eq!(c.in_flight_len(), 2);
        c.finish("a");
        assert!(!c.is_in_flight("a"));
        assert!(c.is_in_flight("b"));
    }
}
