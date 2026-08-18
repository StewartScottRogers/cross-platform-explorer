//! Thumbnail pipeline glue (CPE-1237, epic CPE-718): the seam that drives the `thumb_queue` priority
//! scheduler (CPE-950) and `thumb_cache` in-memory LRU (CPE-939) from a Tauri command — both modules were
//! built + unit-tested but wired into no dispatch until this ticket. Neither module is reimplemented
//! here: [`ThumbQueue`] still owns ordering/dedupe/promotion, [`ThumbCache`] still owns recency + the
//! dual-budget eviction bookkeeping. This module only adds what they deliberately leave out — the actual
//! thumbnail *bytes* (`ThumbCache`'s own docs: "the actual bytes live on disk / elsewhere") — plus the
//! batch-processing loop a command drives.
//!
//! [`run_thumb_batch`] enqueues every request of one call into a **fresh, per-call** [`ThumbQueue`]: cache
//! hits are served immediately (out of queue order — a hit costs nothing to prioritize), then the queue is
//! drained highest-priority-first, computing each miss via a caller-supplied `compute` closure
//! ([`crate::thumbnail::thumbnail_cached`] in production, which itself checks the on-disk cache before
//! decoding). A per-call queue also gives cancellation for free: a scroll-superseded batch is simply
//! abandoned mid-drain (the caller's `cancelled` closure returns `true`) — `ThumbQueue` has no removal API
//! and doesn't need one, since nothing outlives the call that owns it.
//!
//! The in-memory [`ThumbCache`] (recency + count/byte budget) lives in [`ThumbCacheService`], held in
//! Tauri managed state so it — and the decoded bytes it fronts — persists across the many
//! `thumbnails_stream` calls one scroll session issues: a repeat visit to an already-decoded tile is a
//! cache hit, no re-decode. `crate::thumbnail::thumbnail_cached`'s on-disk cache underneath is what
//! survives an app restart (the ticket's "cached across sessions" AC); this in-memory layer is the fast
//! path in front of it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::thumb_cache::{thumb_key, ThumbCache};
use crate::thumb_queue::{Priority, ThumbQueue};

/// Default in-memory hot-cache budget: enough recently-viewed tiles to cover several screens of a
/// gallery grid without re-decoding on a small scroll-back, bounded well under typical app memory (the
/// on-disk cache in [`crate::thumbnail`] is the durable layer; this is just the fast path in front of it).
pub const DEFAULT_MAX_ENTRIES: usize = 1024;
/// 64 MiB of decoded PNG thumbnail bytes.
pub const DEFAULT_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// One requested thumbnail: a source file, the target tile edge, and the priority to enqueue it at if
/// it's not already cached. Sent from the frontend (CPE-1237): the visible window at `Priority::Visible`,
/// the prefetch margin at `Priority::Prefetch`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ThumbRequest {
    pub path: String,
    pub target_px: u32,
    pub priority: Priority,
}

/// One resolved thumbnail, streamed back as it's ready. `data_url` is `None` when the format can't be
/// rendered (unreadable, unsupported, corrupt, oversized) — the frontend falls back to the type icon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ThumbResult {
    pub path: String,
    pub target_px: u32,
    pub data_url: Option<String>,
}

/// [`ThumbCache`] bookkeeping plus the byte payloads it fronts. Kept together because `ThumbCache`
/// deliberately tracks only cost + recency, not bytes (see its docs) — `bytes` is reconciled to whichever
/// keys `cache` still holds after every `put` (eviction has no callback to report victims, so this
/// re-syncs by membership; the cache is small — `DEFAULT_MAX_ENTRIES` — so an O(n) retain is cheap).
#[derive(Debug)]
pub struct ThumbCacheStore {
    cache: ThumbCache,
    bytes: HashMap<String, Vec<u8>>,
}

impl ThumbCacheStore {
    pub fn new(max_entries: usize, max_bytes: u64) -> Self {
        Self { cache: ThumbCache::new(max_entries, max_bytes), bytes: HashMap::new() }
    }

    /// A cache hit's bytes, promoting recency. `None` on a miss.
    fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        self.cache.get(key)?;
        self.bytes.get(key).cloned()
    }

    /// Record a freshly-computed thumbnail, evicting least-recently-used entries per `ThumbCache`'s
    /// budgets, then dropping any byte payload `ThumbCache` no longer tracks.
    fn put(&mut self, key: &str, data: Vec<u8>) {
        self.cache.put(key, data.len() as u64);
        self.bytes.insert(key.to_string(), data);
        let cache = &self.cache;
        self.bytes.retain(|k, _| cache.contains(k));
    }

    /// Current resident entry count (diagnostics/tests).
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// True when nothing is cached yet.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

/// Tauri-managed handle to the shared in-memory thumbnail cache: cheaply `Clone`-able (an `Arc` around
/// the store) so an async command can clone it out of managed state and move it into `spawn_blocking` —
/// mirrors `cpe_server::index_service::IndexService`.
#[derive(Debug, Clone)]
pub struct ThumbCacheService(Arc<Mutex<ThumbCacheStore>>);

impl Default for ThumbCacheService {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ENTRIES, DEFAULT_MAX_BYTES)
    }
}

impl ThumbCacheService {
    pub fn new(max_entries: usize, max_bytes: u64) -> Self {
        Self(Arc::new(Mutex::new(ThumbCacheStore::new(max_entries, max_bytes))))
    }

    /// The shared store, for `run_thumb_batch`.
    pub fn store(&self) -> &Mutex<ThumbCacheStore> {
        &self.0
    }

    /// Entries currently resident in the hot cache (diagnostics/tests).
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    /// True when nothing is cached yet.
    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }
}

/// A stable cache key for `path` at `target_px`, folding in the file's mtime + size (via `thumb_key`) so
/// an edited file is a cache miss. `None` if `path` can't be stat'd (deleted/unreadable) — the caller
/// should emit an immediate icon-fallback result rather than queuing.
fn request_key(path: &str, target_px: u32) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Some(thumb_key(path, mtime_ms, meta.len(), target_px))
}

/// Drive `requests` through the priority queue + shared cache: enqueue every request into a fresh
/// per-call `ThumbQueue` (cache hits are served immediately, ahead of queue order — a hit costs nothing
/// to prioritize), then drain highest-priority-first, computing each miss via `compute` and storing the
/// result into `store` for next time. `emit` is called once per request (hit, computed, or
/// unreadable/failed → `data_url: None`) — exactly `requests.len()` times unless `cancelled` returns
/// `true`, in which case draining stops early and the remaining requests are simply never computed
/// (that's the whole cancellation story: a superseded batch's queue is just abandoned mid-drain —
/// `ThumbQueue` has no removal API and doesn't need one here).
///
/// Pure enough to unit-test with a fake `compute`/`cancelled`/`emit` — no Tauri types appear in this
/// module at all.
pub fn run_thumb_batch(
    requests: &[ThumbRequest],
    store: &Mutex<ThumbCacheStore>,
    mut compute: impl FnMut(&Path, u32) -> Result<Vec<u8>, String>,
    mut cancelled: impl FnMut() -> bool,
    mut emit: impl FnMut(ThumbResult),
) -> usize {
    let mut queue = ThumbQueue::new();
    let mut pending: HashMap<String, &ThumbRequest> = HashMap::new();
    let mut emitted = 0usize;

    for req in requests {
        if cancelled() {
            return emitted;
        }
        let Some(key) = request_key(&req.path, req.target_px) else {
            emit(ThumbResult { path: req.path.clone(), target_px: req.target_px, data_url: None });
            emitted += 1;
            continue;
        };
        if let Some(bytes) = store.lock().unwrap().get(&key) {
            emit(ThumbResult {
                path: req.path.clone(),
                target_px: req.target_px,
                data_url: Some(to_data_url(&bytes)),
            });
            emitted += 1;
            continue;
        }
        // A key already queued (e.g. the same tile appears in both the visible and prefetch slices of
        // one batch) is deduped/promoted by `ThumbQueue::enqueue` itself; `pending` just tracks the
        // latest request seen for it, for `compute`'s sake.
        pending.insert(key.clone(), req);
        queue.enqueue(&key, req.priority);
    }

    while let Some(key) = queue.next() {
        if cancelled() {
            break;
        }
        let Some(req) = pending.get(&key) else { continue };
        let data_url = match compute(Path::new(&req.path), req.target_px) {
            Ok(bytes) => {
                store.lock().unwrap().put(&key, bytes.clone());
                Some(to_data_url(&bytes))
            }
            Err(_) => None,
        };
        emit(ThumbResult { path: req.path.clone(), target_px: req.target_px, data_url });
        emitted += 1;
    }
    emitted
}

fn to_data_url(png: &[u8]) -> String {
    use base64::Engine;
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;

    /// CPE-1693: returns the [`crate::fsutil::ScratchDir`] guard alongside the file path so the
    /// directory is removed on drop instead of relying on each call site's own trailing
    /// `remove_dir_all` (which never runs on a panic). Callers must keep the guard alive for as long as
    /// the file path needs to exist — bind it (`let (_g, f) = scratch_file(..);`), don't discard it.
    fn scratch_file(tag: &str, bytes: &[u8]) -> (crate::fsutil::ScratchDir, std::path::PathBuf) {
        let d = crate::fsutil::scratch_dir(&format!("cpe-thumbpipe-{tag}"));
        let f = d.join("f.bin");
        std::fs::write(&f, bytes).unwrap();
        (d, f)
    }

    #[test]
    fn visible_requests_are_computed_before_prefetch_requests() {
        let (_ga, a) = scratch_file("order-a", b"aaaa");
        let (_gb, b) = scratch_file("order-b", b"bbbb");
        let store = StdMutex::new(ThumbCacheStore::new(16, 1_000_000));
        let requests = vec![
            ThumbRequest { path: b.to_string_lossy().into(), target_px: 32, priority: Priority::Prefetch },
            ThumbRequest { path: a.to_string_lossy().into(), target_px: 32, priority: Priority::Visible },
        ];
        let order = StdMutex::new(Vec::<String>::new());
        run_thumb_batch(
            &requests,
            &store,
            |path, _edge| {
                order.lock().unwrap().push(path.to_string_lossy().into_owned());
                Ok(vec![1, 2, 3])
            },
            || false,
            |_result| {},
        );
        let order = order.into_inner().unwrap();
        assert_eq!(order, vec![a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned()],
            "the Visible request must be computed first even though it was enqueued second");
        let _ = std::fs::remove_dir_all(a.parent().unwrap());
        let _ = std::fs::remove_dir_all(b.parent().unwrap());
    }

    #[test]
    fn a_cache_hit_never_calls_compute_again() {
        let (_gf, f) = scratch_file("hit", b"hello");
        let store = StdMutex::new(ThumbCacheStore::new(16, 1_000_000));
        let req = ThumbRequest { path: f.to_string_lossy().into(), target_px: 64, priority: Priority::Visible };
        let calls = AtomicUsize::new(0);
        let compute = |_p: &Path, _e: u32| {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok::<Vec<u8>, String>(vec![9, 9, 9])
        };
        let mut results = Vec::new();
        run_thumb_batch(std::slice::from_ref(&req), &store, compute, || false, |r| results.push(r));
        assert_eq!(calls.load(Ordering::Relaxed), 1, "first request is a miss -> computed once");
        assert!(results[0].data_url.as_deref().unwrap().starts_with("data:image/png;base64,"));

        // Same request again: served from the cache, `compute` must not run a second time.
        let mut results2 = Vec::new();
        run_thumb_batch(std::slice::from_ref(&req), &store, compute, || false, |r| results2.push(r));
        assert_eq!(calls.load(Ordering::Relaxed), 1, "second request is a cache hit -> compute not called again");
        assert_eq!(results2[0].data_url, results[0].data_url);
        let _ = std::fs::remove_dir_all(f.parent().unwrap());
    }

    #[test]
    fn cancellation_stops_the_drain_early() {
        let (_ga, a) = scratch_file("cancel-a", b"a");
        let (_gb, b) = scratch_file("cancel-b", b"b");
        let store = StdMutex::new(ThumbCacheStore::new(16, 1_000_000));
        let requests = vec![
            ThumbRequest { path: a.to_string_lossy().into(), target_px: 32, priority: Priority::Visible },
            ThumbRequest { path: b.to_string_lossy().into(), target_px: 32, priority: Priority::Visible },
        ];
        let computed = AtomicUsize::new(0);
        // Cancel right after the first compute call, mirroring a stream the frontend has already
        // superseded (a new visible window landed while this batch was still draining).
        let emitted = run_thumb_batch(
            &requests,
            &store,
            |_p, _e| {
                computed.fetch_add(1, Ordering::Relaxed);
                Ok(vec![1])
            },
            || computed.load(Ordering::Relaxed) >= 1,
            |_r| {},
        );
        assert_eq!(computed.load(Ordering::Relaxed), 1, "the second (cancelled) request must never compute");
        assert_eq!(emitted, 1, "only the completed request was emitted");
        let _ = std::fs::remove_dir_all(a.parent().unwrap());
        let _ = std::fs::remove_dir_all(b.parent().unwrap());
    }

    #[test]
    fn an_unreadable_path_falls_back_immediately_without_queuing() {
        let store = StdMutex::new(ThumbCacheStore::new(16, 1_000_000));
        let req = ThumbRequest { path: "/does/not/exist/at/all.png".into(), target_px: 32, priority: Priority::Visible };
        let calls = AtomicUsize::new(0);
        let mut results = Vec::new();
        run_thumb_batch(
            std::slice::from_ref(&req),
            &store,
            |_p, _e| {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(vec![1])
            },
            || false,
            |r| results.push(r),
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0, "an unstat-able path is never queued for compute");
        assert_eq!(results.len(), 1);
        assert!(results[0].data_url.is_none(), "fallback result has no data_url -> frontend shows the type icon");
    }

    #[test]
    fn a_compute_error_yields_a_fallback_result_not_a_panic() {
        let (_gf, f) = scratch_file("err", b"not-an-image");
        let store = StdMutex::new(ThumbCacheStore::new(16, 1_000_000));
        let req = ThumbRequest { path: f.to_string_lossy().into(), target_px: 32, priority: Priority::Visible };
        let mut results = Vec::new();
        run_thumb_batch(
            std::slice::from_ref(&req),
            &store,
            |_p, _e| Err::<Vec<u8>, String>("decode failed".into()),
            || false,
            |r| results.push(r),
        );
        assert_eq!(results.len(), 1);
        assert!(results[0].data_url.is_none());
        assert_eq!(store.into_inner().unwrap().len(), 0, "a failed decode is never cached");
        let _ = std::fs::remove_dir_all(f.parent().unwrap());
    }

    #[test]
    fn cache_eviction_under_the_byte_budget_still_works_end_to_end() {
        // A tiny byte budget forces eviction after a couple of puts — proves `ThumbCacheStore` actually
        // exercises `ThumbCache`'s real eviction (not bypassed) and keeps `bytes` in sync with it.
        let (_ga, a) = scratch_file("evict-a", b"a");
        let (_gb, b) = scratch_file("evict-b", b"b");
        let (_gc, c) = scratch_file("evict-c", b"c");
        let store = StdMutex::new(ThumbCacheStore::new(16, 12)); // ~2 six-byte thumbnails fit, not 3
        let six_bytes = |_p: &Path, _e: u32| Ok::<Vec<u8>, String>(vec![0u8; 6]);
        for p in [&a, &b, &c] {
            let req = ThumbRequest { path: p.to_string_lossy().into(), target_px: 8, priority: Priority::Visible };
            run_thumb_batch(std::slice::from_ref(&req), &store, six_bytes, || false, |_r| {});
        }
        assert!(store.lock().unwrap().len() <= 2, "the byte budget must have evicted the oldest entry");
        for p in [&a, &b, &c] {
            let _ = std::fs::remove_dir_all(p.parent().unwrap());
        }
    }
}
