//! Content-addressed snapshot store (CPE-969, epic CPE-732; also serves CPE-735 local snapshots).
//!
//! The *store side* of checkpoint & rollback. [`crate::restore_plan`] (CPE-917) already computes the revert
//! diff over two [`Snapshot`]s (`path → FileState{hash,size}`); this module owns what the epic's DoD calls
//! "efficient content-addressed snapshots… bounded, dedup": a refcounted content-addressed [`BlobStore`]
//! plus [`plan_capture`], which decides which of a scan's blobs are **new** (must be written) vs **reused**
//! (already present — the dedup win) under a per-file and whole-store byte [`CaptureBudget`], and the
//! [`apply_capture`] / [`release`] pair that drive the refcounted store lifecycle.
//!
//! Pure: a transform over the `Snapshot` maps [`crate::restore_plan`] already defines — no filesystem, no
//! GUI, no new dependencies (std-only), so it's fully cargo-testable and always available (not feature-
//! gated), like `restore_plan` and [`crate::snapshot_retention`]. The bytes behind each hash are the
//! caller's to persist; this module tracks *which* blobs exist, how big they are, and how many snapshots
//! reference them, so dedup is a lookup and garbage collection is refcount-driven.

use std::collections::{BTreeMap, BTreeSet};

use crate::restore_plan::Snapshot;

/// A stored blob's bookkeeping: its byte size and how many snapshots currently reference it. A blob is
/// removed (its bytes freed) when `refs` reaches 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobMeta {
    pub size: u64,
    pub refs: u32,
}

/// A content-addressed blob store's index: content hash → [`BlobMeta`]. Identical content — whether it
/// appears under many paths in one snapshot or is shared across snapshots — is a single blob, so the
/// store's footprint counts unique content only. A `BTreeMap` keeps iteration deterministic.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlobStore {
    blobs: BTreeMap<String, BlobMeta>,
}

impl BlobStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the store already holds a blob for `hash` (the dedup test).
    pub fn contains(&self, hash: &str) -> bool {
        self.blobs.contains_key(hash)
    }

    /// The metadata for `hash`, if present.
    pub fn get(&self, hash: &str) -> Option<BlobMeta> {
        self.blobs.get(hash).copied()
    }

    /// How many distinct blobs the store holds.
    pub fn blob_count(&self) -> usize {
        self.blobs.len()
    }

    /// Whether the store holds no blobs.
    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }

    /// The store's total footprint — the sum of unique blob sizes (reused content counted once).
    pub fn total_bytes(&self) -> u64 {
        self.blobs.values().map(|m| m.size).sum()
    }

    /// Iterate `(hash, meta)` in deterministic hash order — for retention/GC callers.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &BlobMeta)> {
        self.blobs.iter()
    }
}

/// The bounded-store rule. A value of `0` means "no limit" for that dimension, so the default budget is
/// unbounded and existing callers opt into a cap explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaptureBudget {
    /// Skip any single file larger than this (don't snapshot huge blobs). `0` = no per-file cap.
    pub max_blob_bytes: u64,
    /// Cap the whole store's footprint: a new blob that would push [`BlobStore::total_bytes`] past this is
    /// skipped. `0` = no store cap. Reused blobs are already counted, so they never breach it.
    pub max_total_bytes: u64,
}

impl CaptureBudget {
    /// An explicitly unbounded budget.
    pub const UNLIMITED: CaptureBudget = CaptureBudget { max_blob_bytes: 0, max_total_bytes: 0 };
}

/// A distinct blob a capture touches: its content hash and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRef {
    pub hash: String,
    pub size: u64,
}

/// Why a file's content was left out of a capture — surfaced (never silently dropped) so the caller can
/// warn that a checkpoint is incomplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The file is larger than [`CaptureBudget::max_blob_bytes`].
    Oversize,
    /// Storing this new blob would push the store past [`CaptureBudget::max_total_bytes`].
    Budget,
}

/// A skipped file: the path that owned the content, its size, and why it was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedFile {
    pub path: String,
    pub size: u64,
    pub reason: SkipReason,
}

/// The plan for capturing a scan against a store: which blobs are new, which are reused (dedup), and which
/// files were skipped by the budget. Deterministic (all vectors sorted).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapturePlan {
    /// New blobs not already in the store — their bytes must be written.
    pub to_store: Vec<BlobRef>,
    /// Blobs already in the store — reused, nothing to write (the dedup win).
    pub reused: Vec<BlobRef>,
    /// Files whose content was skipped (oversize or over budget), with the reason.
    pub skipped: Vec<SkippedFile>,
    /// Bytes this capture adds to the store (sum of `to_store` sizes).
    pub added_bytes: u64,
}

impl CapturePlan {
    /// Whether the capture writes no new blobs (everything was reused or skipped).
    pub fn stores_nothing(&self) -> bool {
        self.to_store.is_empty()
    }

    /// The set of blob hashes this capture references (new + reused) — what [`apply_capture`] will bump and
    /// what the caller records as the snapshot's manifest to later [`release`].
    pub fn referenced_hashes(&self) -> BTreeSet<String> {
        self.to_store
            .iter()
            .chain(self.reused.iter())
            .map(|b| b.hash.clone())
            .collect()
    }
}

/// Plan a capture of `scan` against `store` under `budget`, without mutating the store. Dedups the scan to
/// distinct content hashes, splits them into new (`to_store`) vs already-present (`reused`), and records
/// any files skipped by the per-file or whole-store byte cap.
///
/// Determinism: paths are visited in sorted order so the *first* path owning a given new blob is the one
/// blamed if that blob is later skipped by the store cap, and blobs are processed hash-sorted.
pub fn plan_capture(store: &BlobStore, scan: &Snapshot, budget: &CaptureBudget) -> CapturePlan {
    // Reduce the scan to distinct content: hash → (size, first path seen). `scan` is a BTreeMap, so paths
    // arrive sorted and "first path" is stable.
    let mut distinct: BTreeMap<&str, (u64, &str)> = BTreeMap::new();
    for (path, state) in scan {
        distinct.entry(&state.hash).or_insert((state.size, path));
    }

    let mut plan = CapturePlan::default();
    // Track the projected footprint as we admit new blobs, so the store cap accounts for this capture's own
    // additions, not just the pre-existing store size.
    let mut projected_total = store.total_bytes();

    for (hash, (size, path)) in distinct {
        if store.contains(hash) {
            // Already stored — pure dedup, no budget interaction (its bytes are already counted).
            plan.reused.push(BlobRef { hash: hash.to_string(), size });
            continue;
        }
        // A brand-new blob. Per-file cap first (an oversize file is never stored, regardless of room).
        if budget.max_blob_bytes != 0 && size > budget.max_blob_bytes {
            plan.skipped.push(SkippedFile { path: path.to_string(), size, reason: SkipReason::Oversize });
            continue;
        }
        // Whole-store cap: would admitting this blob breach it?
        if budget.max_total_bytes != 0 && projected_total.saturating_add(size) > budget.max_total_bytes {
            plan.skipped.push(SkippedFile { path: path.to_string(), size, reason: SkipReason::Budget });
            continue;
        }
        projected_total = projected_total.saturating_add(size);
        plan.added_bytes = plan.added_bytes.saturating_add(size);
        plan.to_store.push(BlobRef { hash: hash.to_string(), size });
    }

    plan
}

/// Commit `plan` to `store`: insert each new blob with one reference, and bump the reference count on each
/// reused blob. After this, the referenced blobs are held by one more snapshot. Skipped files are not
/// referenced (their content isn't in the store), so the snapshot's true manifest is
/// [`CapturePlan::referenced_hashes`].
pub fn apply_capture(store: &mut BlobStore, plan: &CapturePlan) {
    for b in &plan.to_store {
        // A new hash — but guard against a double-apply by folding into any existing entry.
        let meta = store.blobs.entry(b.hash.clone()).or_insert(BlobMeta { size: b.size, refs: 0 });
        meta.refs = meta.refs.saturating_add(1);
    }
    for b in &plan.reused {
        if let Some(meta) = store.blobs.get_mut(&b.hash) {
            meta.refs = meta.refs.saturating_add(1);
        }
    }
}

/// Release a snapshot's hold on `hashes` (e.g. when retention thins it away, see
/// [`crate::snapshot_retention`]): decrement each blob's ref count and garbage-collect any that reach 0.
/// Returns the bytes freed (sum of GC'd blob sizes). Unknown hashes are ignored.
pub fn release(store: &mut BlobStore, hashes: &BTreeSet<String>) -> u64 {
    let mut freed = 0u64;
    for hash in hashes {
        let Some(meta) = store.blobs.get_mut(hash) else { continue };
        meta.refs = meta.refs.saturating_sub(1);
        if meta.refs == 0 {
            freed = freed.saturating_add(meta.size);
            store.blobs.remove(hash);
        }
    }
    freed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore_plan::FileState;

    /// Build a `Snapshot` from `(path, hash, size)` tuples.
    fn snap(entries: &[(&str, &str, u64)]) -> Snapshot {
        entries.iter().map(|(p, h, sz)| (p.to_string(), FileState::new(*h, *sz))).collect()
    }

    fn hashes(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plan_dedups_repeated_content_within_a_scan() {
        // Three paths, two of them identical content ("h1") → one blob to store, sized once.
        let scan = snap(&[("a", "h1", 10), ("b", "h1", 10), ("c", "h2", 20)]);
        let plan = plan_capture(&BlobStore::new(), &scan, &CaptureBudget::UNLIMITED);
        assert_eq!(plan.to_store.len(), 2, "h1 stored once despite two paths");
        assert!(plan.reused.is_empty());
        assert_eq!(plan.added_bytes, 30); // 10 + 20, not 40
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn plan_reuses_blobs_already_in_the_store() {
        let mut store = BlobStore::new();
        let first = plan_capture(&store, &snap(&[("a", "h1", 10)]), &CaptureBudget::UNLIMITED);
        apply_capture(&mut store, &first);
        // Second snapshot re-references h1 and adds h2.
        let plan = plan_capture(&store, &snap(&[("a", "h1", 10), ("b", "h2", 5)]), &CaptureBudget::UNLIMITED);
        assert_eq!(plan.reused.iter().map(|b| b.hash.as_str()).collect::<Vec<_>>(), vec!["h1"]);
        assert_eq!(plan.to_store.iter().map(|b| b.hash.as_str()).collect::<Vec<_>>(), vec!["h2"]);
        assert_eq!(plan.added_bytes, 5);
    }

    #[test]
    fn apply_dedups_store_footprint_across_snapshots() {
        let mut store = BlobStore::new();
        // Two snapshots sharing h1; the store holds h1 once but with refs=2.
        let p1 = plan_capture(&store, &snap(&[("a", "h1", 100)]), &CaptureBudget::UNLIMITED);
        apply_capture(&mut store, &p1);
        let p2 = plan_capture(&store, &snap(&[("b", "h1", 100)]), &CaptureBudget::UNLIMITED);
        apply_capture(&mut store, &p2);
        assert_eq!(store.blob_count(), 1);
        assert_eq!(store.total_bytes(), 100); // counted once
        assert_eq!(store.get("h1").unwrap().refs, 2);
    }

    #[test]
    fn per_file_cap_skips_oversize_content() {
        let scan = snap(&[("small", "h1", 10), ("huge", "h2", 10_000)]);
        let budget = CaptureBudget { max_blob_bytes: 1_000, max_total_bytes: 0 };
        let plan = plan_capture(&BlobStore::new(), &scan, &budget);
        assert_eq!(plan.to_store.iter().map(|b| b.hash.as_str()).collect::<Vec<_>>(), vec!["h1"]);
        assert_eq!(plan.skipped, vec![SkippedFile { path: "huge".into(), size: 10_000, reason: SkipReason::Oversize }]);
    }

    #[test]
    fn store_cap_skips_new_blobs_that_would_overflow() {
        // Store cap 25; h1(20) fits, h2(10) would push to 30 → skipped for Budget. Hash order a<b<c drives it.
        let scan = snap(&[("a", "h1", 20), ("b", "h2", 10)]);
        let budget = CaptureBudget { max_blob_bytes: 0, max_total_bytes: 25 };
        let plan = plan_capture(&BlobStore::new(), &scan, &budget);
        assert_eq!(plan.to_store.iter().map(|b| b.hash.as_str()).collect::<Vec<_>>(), vec!["h1"]);
        assert_eq!(plan.skipped, vec![SkippedFile { path: "b".into(), size: 10, reason: SkipReason::Budget }]);
        assert_eq!(plan.added_bytes, 20);
    }

    #[test]
    fn reused_blobs_do_not_count_against_the_store_cap() {
        let mut store = BlobStore::new();
        let seed = plan_capture(&store, &snap(&[("a", "h1", 20)]), &CaptureBudget::UNLIMITED);
        apply_capture(&mut store, &seed);
        // Store already at 20 with a tight cap of 20; re-referencing h1 must still succeed (it's reused).
        let budget = CaptureBudget { max_blob_bytes: 0, max_total_bytes: 20 };
        let plan = plan_capture(&store, &snap(&[("a", "h1", 20)]), &budget);
        assert_eq!(plan.reused.len(), 1);
        assert!(plan.to_store.is_empty());
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn release_gcs_unreferenced_blobs_and_frees_bytes() {
        let mut store = BlobStore::new();
        let plan = plan_capture(&store, &snap(&[("a", "h1", 30), ("b", "h2", 12)]), &CaptureBudget::UNLIMITED);
        apply_capture(&mut store, &plan);
        assert_eq!(store.total_bytes(), 42);
        // Releasing this snapshot's blobs drops both to 0 refs → GC'd, 42 bytes freed.
        let freed = release(&mut store, &plan.referenced_hashes());
        assert_eq!(freed, 42);
        assert!(store.is_empty());
    }

    #[test]
    fn a_shared_blob_survives_releasing_one_snapshot() {
        let mut store = BlobStore::new();
        let a = plan_capture(&store, &snap(&[("a", "h1", 8)]), &CaptureBudget::UNLIMITED);
        apply_capture(&mut store, &a);
        let b = plan_capture(&store, &snap(&[("b", "h1", 8), ("c", "h2", 4)]), &CaptureBudget::UNLIMITED);
        apply_capture(&mut store, &b);
        // Release snapshot A: h1 still held by B (refs 2→1), h1 survives; nothing freed.
        let freed = release(&mut store, &a.referenced_hashes());
        assert_eq!(freed, 0);
        assert!(store.contains("h1"));
        assert_eq!(store.get("h1").unwrap().refs, 1);
        // Now release B: h1 (1→0) and h2 (1→0) GC'd, 12 bytes freed.
        let freed_b = release(&mut store, &b.referenced_hashes());
        assert_eq!(freed_b, 12);
        assert!(store.is_empty());
    }

    #[test]
    fn capture_then_release_round_trips_the_store() {
        // Property: apply a capture, then release exactly its manifest → the store returns to its prior state.
        let mut store = BlobStore::new();
        let seed = plan_capture(&store, &snap(&[("keep", "base", 5)]), &CaptureBudget::UNLIMITED);
        apply_capture(&mut store, &seed);
        let before = store.clone();
        let plan = plan_capture(&store, &snap(&[("x", "hx", 7), ("y", "hy", 9)]), &CaptureBudget::UNLIMITED);
        apply_capture(&mut store, &plan);
        assert_ne!(store, before);
        release(&mut store, &plan.referenced_hashes());
        assert_eq!(store, before, "release undoes a capture's footprint");
    }

    #[test]
    fn empty_scan_captures_nothing() {
        let plan = plan_capture(&BlobStore::new(), &snap(&[]), &CaptureBudget::UNLIMITED);
        assert!(plan.stores_nothing());
        assert!(plan.referenced_hashes().is_empty());
    }

    #[test]
    fn release_ignores_unknown_hashes() {
        let mut store = BlobStore::new();
        let seed = plan_capture(&store, &snap(&[("a", "h1", 3)]), &CaptureBudget::UNLIMITED);
        apply_capture(&mut store, &seed);
        assert_eq!(release(&mut store, &hashes(&["nope", "gone"])), 0);
        assert_eq!(store.total_bytes(), 3); // untouched
    }
}
