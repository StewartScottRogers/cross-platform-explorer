//! Live instant-index **service** (CPE-1137, epic CPE-703): the thin state layer that holds the resident
//! per-volume [`Index`]es and backs the `index_*` Tauri commands. The heavy lifting — the crawl, the
//! on-disk format, the trigram-pruned search — all lives in [`crate::index`] / [`crate::index_query`];
//! this module only *owns* the built indices, persists them, and merges a query across every resident
//! volume. The Tauri app holds one of these in managed state and dispatches the commands into it (the
//! `ipc::Channel` transport stays in the app adapter, per `docs/design/STREAMING.md`).
//!
//! ## Off means off (the epic tiebreaker + this ticket's hard DoD)
//! A freshly-constructed [`IndexService`] touches **no disk** and holds **zero** volumes. Nothing crawls
//! or loads until [`IndexService::build_root`] (an explicit build) or [`IndexService::search_all`] (which
//! lazily loads any persisted-but-not-resident volume from the index directory) is called. With the
//! feature unused there is no background thread, no timer, and no allocation beyond an empty map.
//!
//! Feature-gated behind `index` (same gate as [`crate::index`]): the plain `cpe-server` build compiles
//! zero indexer code.

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::index::{BuildStats, Index, IndexHit};
use crate::index_query;

/// A batch size for streaming ranked hits back to the frontend — small enough that the first rows paint
/// immediately, big enough that a typical result set is one or two flushes (mirrors the 32-hit search
/// batch of the name-search stream, `docs/design/STREAMING.md`).
pub const SEARCH_BATCH: usize = 32;

/// One resident volume's state, for `index_status` (so the UI can show what's indexed). Serialisable —
/// it's an `index_status` command return type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct VolumeStatus {
    pub volume_id: u64,
    /// Live entry count (tombstones excluded).
    pub entries: u64,
    /// Whether the crawl that produced this volume was truncated (dir cap or cancellation).
    pub truncated: bool,
}

/// Holds the loaded instant-search indices, one per volume id. Cheaply cloneable (an `Arc` around the
/// map) so an async Tauri command can clone the handle out of managed state and move it into
/// `spawn_blocking` — the crawl/search never runs on the UI thread (the async-all-commands rule).
///
/// The map is the whole state: an empty map is a fully-off service (see the module docs).
#[derive(Clone, Default)]
pub struct IndexService {
    volumes: Arc<Mutex<HashMap<u64, Index>>>,
}

impl IndexService {
    /// A fresh, empty service. Does **no** disk work and holds no volumes (off means off).
    pub fn new() -> Self {
        Self::default()
    }

    /// How many volumes are resident in memory right now. `0` for a fresh service.
    pub fn resident_count(&self) -> usize {
        self.volumes.lock().unwrap().len()
    }

    /// The on-disk path for a volume's index file under `dir`: `dir/<volume_id>.idx`.
    pub fn volume_path(dir: &Path, volume_id: u64) -> PathBuf {
        dir.join(format!("{volume_id}.idx"))
    }

    /// Crawl `root` into a fresh [`Index`] for `volume_id`, make it resident (replacing any prior index
    /// for that volume), and persist it to `dir/<volume_id>.idx`. `cancel` aborts a long crawl (the
    /// partial index is still stored). `progress` is called with the running [`BuildStats`] as the crawl
    /// advances, so a streaming caller can report live progress. Returns the final stats.
    pub fn build_root(
        &self,
        root: &str,
        volume_id: u64,
        dir: &Path,
        cancel: &AtomicBool,
        progress: impl FnMut(BuildStats),
    ) -> Result<BuildStats, String> {
        let (idx, stats) = Index::build_with(root, volume_id, cancel, progress)?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        idx.save(&Self::volume_path(dir, volume_id))
            .map_err(|e| e.to_string())?;
        self.volumes.lock().unwrap().insert(volume_id, idx);
        Ok(stats)
    }

    /// Bring any volume that has a persisted `*.idx` file under `dir` but isn't yet resident into memory.
    /// A malformed/stale file is skipped (the caller rebuilds), never fatal. Called by the search path so
    /// a build → drop-from-memory → search round-trip transparently reloads from disk.
    fn load_missing(&self, dir: &Path) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut vols = self.volumes.lock().unwrap();
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("idx") {
                continue;
            }
            let Some(vid) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u64>().ok())
            else {
                continue;
            };
            if vols.contains_key(&vid) {
                continue;
            }
            if let Ok(idx) = Index::load(&path) {
                vols.insert(vid, idx);
            }
        }
    }

    /// Parse `input` and search **every resident volume** (lazily loading any persisted-but-not-resident
    /// volume from `dir` first), merging and ranking the hits into one best-first list of up to `limit`.
    /// The shared search logic behind both the collect-to-vec command and the streamed one.
    pub fn search_all(&self, dir: &Path, input: &str, limit: usize) -> Vec<IndexHit> {
        let query = index_query::parse(input);
        if query.is_empty() {
            return Vec::new();
        }
        self.load_missing(dir);
        let vols = self.volumes.lock().unwrap();
        let mut hits: Vec<IndexHit> = Vec::new();
        for idx in vols.values() {
            // Ask each volume for up to `limit`; the cross-volume merge below re-ranks + re-truncates.
            hits.extend(idx.search(&query, limit));
        }
        // Merge-rank across volumes with the same deterministic total order `Index::search` uses.
        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.path.cmp(&b.path))
        });
        hits.truncate(limit);
        hits
    }

    /// Streamed search: run [`IndexService::search_all`] and hand the ranked hits to `flush` in
    /// [`SEARCH_BATCH`]-sized batches (honouring an early `Break`), so the frontend paints the top hits
    /// immediately. Both variants share the one `search_all` ranking, so they can never diverge.
    pub fn stream_search(
        &self,
        dir: &Path,
        input: &str,
        limit: usize,
        batch: usize,
        mut flush: impl FnMut(Vec<IndexHit>) -> ControlFlow<()>,
    ) {
        let hits = self.search_all(dir, input, limit);
        let batch = batch.max(1);
        for chunk in hits.chunks(batch) {
            if let ControlFlow::Break(()) = flush(chunk.to_vec()) {
                break;
            }
        }
    }

    /// A status line per resident volume (entry count + truncated flag), for `index_status`. Reports only
    /// what is **resident** — it does no disk scan, so a fresh service reports an empty list.
    pub fn status(&self) -> Vec<VolumeStatus> {
        let vols = self.volumes.lock().unwrap();
        let mut out: Vec<VolumeStatus> = vols
            .iter()
            .map(|(&volume_id, idx)| VolumeStatus {
                volume_id,
                entries: idx.len() as u64,
                truncated: idx.truncated(),
            })
            .collect();
        out.sort_by_key(|s| s.volume_id); // deterministic order for the UI + tests
        out
    }

    /// Drop `volume_id` from memory and delete its on-disk file under `dir`. Returns whether the volume
    /// was resident. The file delete is best-effort (a missing file is fine).
    pub fn drop_volume(&self, dir: &Path, volume_id: u64) -> bool {
        let removed = self.volumes.lock().unwrap().remove(&volume_id).is_some();
        let _ = std::fs::remove_file(Self::volume_path(dir, volume_id));
        removed
    }

    /// Free **all** resident volumes from memory. On-disk files are left in place (a later search reloads
    /// them) — use [`IndexService::drop_volume`] to also delete a volume's file.
    pub fn clear(&self) {
        self.volumes.lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::Ordering;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::AtomicU64;
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-idxsvc-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// A small tree to crawl: report.rs / report.md across two folders + a README.
    fn sample_tree(root: &Path) {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("README.md"), b"x").unwrap();
        fs::write(root.join("src/main.rs"), b"x").unwrap();
        fs::write(root.join("src/report.rs"), b"x").unwrap();
        fs::write(root.join("docs/report.md"), b"x").unwrap();
    }

    fn names(hits: &[IndexHit]) -> Vec<String> {
        let mut v: Vec<String> = hits.iter().map(|h| h.name.clone()).collect();
        v.sort();
        v
    }

    /// Off-means-off: a fresh service is empty and does no disk work at construction.
    #[test]
    fn fresh_service_is_empty_and_touches_nothing() {
        let svc = IndexService::new();
        assert_eq!(svc.resident_count(), 0);
        assert!(svc.status().is_empty());
        // A search against a directory that doesn't even exist must not panic or create anything.
        let missing = std::env::temp_dir().join("cpe-idxsvc-does-not-exist-xyz");
        assert!(svc.search_all(&missing, "anything", 10).is_empty());
        assert!(!missing.exists(), "search must not create the index dir");
        assert_eq!(svc.resident_count(), 0);
    }

    /// An empty query returns nothing (and doesn't even hit disk).
    #[test]
    fn empty_query_returns_nothing() {
        let d = scratch("empty");
        let svc = IndexService::new();
        assert!(svc.search_all(&d, "   ", 10).is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    /// build_root → search_all returns ranked cross-folder hits, and progress is reported.
    #[test]
    fn build_then_search_returns_ranked_cross_folder_hits() {
        let tree = scratch("tree");
        sample_tree(&tree);
        let idxdir = scratch("idxdir");
        let svc = IndexService::new();

        let mut ticks = 0u32;
        let stats = svc
            .build_root(
                &tree.to_string_lossy(),
                1,
                &idxdir,
                &AtomicBool::new(false),
                |_| ticks += 1,
            )
            .unwrap();
        assert!(!stats.truncated);
        assert!(ticks >= 1, "progress callback should fire at least the final tick");
        assert_eq!(svc.resident_count(), 1);

        let hits = svc.search_all(&idxdir, "report", 10);
        assert_eq!(names(&hits), vec!["report.md", "report.rs"]);
        // The persisted file exists at dir/<volume_id>.idx.
        assert!(IndexService::volume_path(&idxdir, 1).is_file());

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    /// ext:/path:/name-term queries parse + filter correctly through the service path.
    #[test]
    fn structured_filters_flow_through_the_service() {
        let tree = scratch("filters");
        sample_tree(&tree);
        let idxdir = scratch("filters-idx");
        let svc = IndexService::new();
        svc.build_root(&tree.to_string_lossy(), 7, &idxdir, &AtomicBool::new(false), |_| {})
            .unwrap();

        let rs = svc.search_all(&idxdir, "report ext:rs", 10);
        assert_eq!(names(&rs), vec!["report.rs"]);
        let docs = svc.search_all(&idxdir, "report path:docs", 10);
        assert_eq!(names(&docs), vec!["report.md"]);
        let globbed = svc.search_all(&idxdir, "*.rs", 10);
        assert_eq!(names(&globbed), vec!["main.rs", "report.rs"]);

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    /// Persistence round-trip: build → drop-from-memory → search auto-loads from disk → same hits.
    #[test]
    fn search_reloads_dropped_volume_from_disk() {
        let tree = scratch("persist");
        sample_tree(&tree);
        let idxdir = scratch("persist-idx");
        let svc = IndexService::new();
        svc.build_root(&tree.to_string_lossy(), 3, &idxdir, &AtomicBool::new(false), |_| {})
            .unwrap();
        let before = svc.search_all(&idxdir, "report", 10);

        // Free memory but keep the on-disk file (clear, not drop).
        svc.clear();
        assert_eq!(svc.resident_count(), 0);

        // Searching again must transparently reload the persisted volume and return the same hits.
        let after = svc.search_all(&idxdir, "report", 10);
        assert_eq!(names(&before), names(&after));
        assert_eq!(svc.resident_count(), 1, "search should have reloaded the volume");

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    /// The streamed variant yields exactly the collect-to-vec ranking, batched.
    #[test]
    fn stream_search_matches_collect_variant() {
        let tree = scratch("stream");
        sample_tree(&tree);
        let idxdir = scratch("stream-idx");
        let svc = IndexService::new();
        svc.build_root(&tree.to_string_lossy(), 9, &idxdir, &AtomicBool::new(false), |_| {})
            .unwrap();

        let collected = svc.search_all(&idxdir, "report", 10);
        let mut streamed: Vec<IndexHit> = Vec::new();
        svc.stream_search(&idxdir, "report", 10, 1, |b| {
            streamed.extend(b);
            ControlFlow::Continue(())
        });
        assert_eq!(collected, streamed);

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }

    /// index_status reports resident volumes + counts; drop frees the volume and removes its file.
    #[test]
    fn status_and_drop_behave() {
        let tree = scratch("status");
        sample_tree(&tree);
        let idxdir = scratch("status-idx");
        let svc = IndexService::new();
        svc.build_root(&tree.to_string_lossy(), 5, &idxdir, &AtomicBool::new(false), |_| {})
            .unwrap();

        let status = svc.status();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].volume_id, 5);
        assert!(status[0].entries > 0);
        assert!(!status[0].truncated);

        let file = IndexService::volume_path(&idxdir, 5);
        assert!(file.is_file());
        assert!(svc.drop_volume(&idxdir, 5));
        assert_eq!(svc.resident_count(), 0);
        assert!(!file.exists(), "drop should delete the on-disk file");
        // Dropping again is a no-op (not resident).
        assert!(!svc.drop_volume(&idxdir, 5));

        let _ = fs::remove_dir_all(&tree);
        let _ = fs::remove_dir_all(&idxdir);
    }
}
