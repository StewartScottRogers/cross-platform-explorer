//! Retention-prune command layer (CPE-1196, epic CPE-735): wires the pure grandfather-father-son policy
//! ([`crate::snapshot_retention::thin`]) to the real on-disk snapshot store
//! ([`crate::snapshot_capture`]) — enumerate a root's manifests, decide keep/prune under a
//! [`RetentionPolicy`], and (only in [`apply`]) actually [`crate::snapshot_capture::prune`] the losers.
//!
//! Store-dir-based, like [`crate::snapshot_capture`] itself (no [`crate::ctx::ServerCtx`] here) — the
//! per-root store-dir resolution stays in [`crate::checkpoint_store`], which is the natural place for a
//! ctx-aware `root -> store_dir` wrapper around these functions.
//!
//! [`preview`] is read-only: it never calls [`crate::snapshot_capture::prune`], so it's always safe to call
//! before showing the user what would happen. [`apply`] is the only function here that touches disk, and it
//! reuses [`crate::snapshot_capture::prune`] verbatim — the manifest-deleted-first / leak-over-corruption
//! invariant documented at `snapshot_capture.rs:218-247` is untouched; this module never opens a manifest,
//! an index, or a blob file directly.

use std::collections::BTreeMap;

use crate::snapshot_capture;
use crate::snapshot_retention::{thin, RetentionPolicy, Snapshot as RetentionSnapshot};

/// The keep/prune verdict for a store under a policy, plus the store's current footprint — non-destructive,
/// safe to compute and show before [`apply`] touches anything.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RetentionPreview {
    /// Manifest ids the policy would keep, newest-first.
    pub keep: Vec<String>,
    /// Manifest ids the policy would prune, oldest-first.
    pub prune: Vec<String>,
    /// The store's current total footprint in bytes (informational only — `preview` never mutates it).
    pub total_bytes: u64,
}

/// The outcome of actually pruning a store: which manifests survived, which were removed (by the GFS
/// policy and/or the optional byte cap), and how many bytes were freed.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RetentionApplyResult {
    /// Manifest ids still present after pruning, newest-first.
    pub kept: Vec<String>,
    /// Manifest ids actually removed — the GFS losers, plus any byte-cap eviction beyond them.
    pub pruned: Vec<String>,
    /// Total bytes freed across every prune in this apply.
    pub bytes_freed: u64,
}

/// Load every manifest in `store_dir` as a [`RetentionSnapshot`] (`id` + `epoch_s`) for
/// [`crate::snapshot_retention::thin`] — `created_ms` truncated to seconds, since retention buckets
/// (hourly/daily/weekly/monthly) don't need sub-second resolution.
fn manifests_as_snapshots(store_dir: &str) -> Result<Vec<RetentionSnapshot>, String> {
    Ok(snapshot_capture::list_manifests(store_dir)?
        .into_iter()
        .map(|m| RetentionSnapshot { id: m.id, epoch_s: m.created_ms / 1000 })
        .collect())
}

/// Preview what [`apply`] would do to `store_dir` under `policy`, without touching disk: enumerate its
/// manifests, run [`thin`], and report the keep/prune split plus the store's current footprint.
pub fn preview(store_dir: &str, policy: &RetentionPolicy) -> Result<RetentionPreview, String> {
    let snaps = manifests_as_snapshots(store_dir)?;
    let result = thin(&snaps, policy);
    let total_bytes = snapshot_capture::store_total_bytes(store_dir)?;
    Ok(RetentionPreview { keep: result.keep, prune: result.prune, total_bytes })
}

/// Actually prune `store_dir` to `policy`: run the same [`thin`] decision [`preview`] would, then
/// [`crate::snapshot_capture::prune`] every losing manifest. If `max_total_bytes` is `Some` (and nonzero),
/// after the GFS pass the survivors are further thinned **oldest-first** — protecting the newest snapshots
/// last — until the store's footprint is at or under the cap or only one survivor remains (a policy is
/// never allowed to prune a store down to zero snapshots; that would make `max_total_bytes` a silent
/// full-wipe knob instead of a cap).
pub fn apply(
    store_dir: &str,
    policy: &RetentionPolicy,
    max_total_bytes: Option<u64>,
) -> Result<RetentionApplyResult, String> {
    let snaps = manifests_as_snapshots(store_dir)?;
    let mut result = thin(&snaps, policy);

    // Floor: NEVER prune a non-empty store down to zero snapshots. `thin()` returns an empty `keep`
    // whenever the policy retains 0 across all four GFS tiers ({0,0,0,0}), which the GFS loop below
    // would then execute as a full wipe — silent, irreversible data loss (a zeroed schedule-rule
    // retention would erase every snapshot it just captured). This guard mirrors the `kept.len() <= 1`
    // floor already present in the byte-cap branch: keep the single NEWEST manifest so at least one
    // snapshot always survives. (Ties broken by id for determinism.)
    if result.keep.is_empty() {
        if let Some(newest) = snaps.iter().max_by_key(|s| (s.epoch_s, s.id.clone())) {
            let id = newest.id.clone();
            result.prune.retain(|p| p != &id);
            result.keep.push(id);
        }
    }

    let mut bytes_freed = 0u64;
    let mut pruned = Vec::new();
    for id in &result.prune {
        bytes_freed += snapshot_capture::prune(store_dir, id)?;
        pruned.push(id.clone());
    }

    let mut kept = result.keep;

    if let Some(cap) = max_total_bytes {
        if cap > 0 {
            // Oldest-first among survivors: `created_ms` looked up from the snapshots already loaded
            // above (before any pruning changed the on-disk set).
            let created_by_id: BTreeMap<&str, u64> =
                snaps.iter().map(|s| (s.id.as_str(), s.epoch_s)).collect();
            let mut oldest_first = kept.clone();
            oldest_first.sort_by_key(|id| created_by_id.get(id.as_str()).copied().unwrap_or(0));

            let mut total = snapshot_capture::store_total_bytes(store_dir)?;
            for id in &oldest_first {
                if total <= cap || kept.len() <= 1 {
                    break;
                }
                let freed = snapshot_capture::prune(store_dir, id)?;
                bytes_freed += freed;
                total = total.saturating_sub(freed);
                kept.retain(|k| k != id);
                pruned.push(id.clone());
            }
        }
    }

    Ok(RetentionApplyResult { kept, pruned, bytes_freed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::CaptureBudget;
    use crate::snapshot_capture::capture;
    use std::fs;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-snapprune-{tag}"))
    }

    /// Capture `src` into `store` and then hand-edit the resulting manifest's `created_ms` to `epoch_s *
    /// 1000`, so tests can place captures at arbitrary spread timestamps without sleeping. Returns the
    /// manifest id.
    fn capture_at(src: &std::path::Path, store: &std::path::Path, epoch_s: u64) -> String {
        let outcome =
            capture(&src.to_string_lossy(), &store.to_string_lossy(), &CaptureBudget::UNLIMITED).unwrap();
        let path = store.join("manifests").join(format!("{}.json", outcome.manifest_id));
        let mut doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        doc["created_ms"] = serde_json::json!(epoch_s * 1000);
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        outcome.manifest_id
    }

    fn all_pol() -> RetentionPolicy {
        RetentionPolicy { hourly: 2, daily: 0, weekly: 0, monthly: 0 }
    }

    #[test]
    fn preview_is_non_destructive() {
        let src = scratch("prev-src");
        let store = scratch("prev-store");
        fs::write(src.join("a.txt"), b"a").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"a2").unwrap();
        let _m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), b"a3").unwrap();
        let m3 = capture_at(&src, &store, 3 * 3600);

        let before = snapshot_capture::list_manifests(&store.to_string_lossy()).unwrap().len();
        let preview = preview(&store.to_string_lossy(), &all_pol()).unwrap();
        assert_eq!(preview.prune, vec![m1.clone()]);
        assert!(preview.keep.contains(&m3));

        // Nothing on disk changed: same manifest count, and the "pruned" one still restores.
        let after = snapshot_capture::list_manifests(&store.to_string_lossy()).unwrap().len();
        assert_eq!(before, after, "preview must not touch the manifests dir");
        assert!(
            snapshot_capture::restore(&store.to_string_lossy(), &m1, &scratch("prev-restore").to_string_lossy())
                .is_ok(),
            "the manifest preview marked for pruning is untouched by preview() and still restores"
        );

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn apply_keeps_gfs_survivors_and_they_still_restore_byte_for_byte() {
        let src = scratch("apply-src");
        let store = scratch("apply-store");
        fs::write(src.join("a.txt"), b"v1").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"v2").unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), b"v3").unwrap();
        let m3 = capture_at(&src, &store, 3 * 3600);

        let bytes_before = snapshot_capture::store_total_bytes(&store.to_string_lossy()).unwrap();

        // hourly=2 keeps the two newest hour buckets (m2, m3), prunes m1.
        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap();
        assert_eq!(result.pruned, vec![m1.clone()]);
        assert!(result.kept.contains(&m2) && result.kept.contains(&m3));
        assert!(result.bytes_freed > 0);

        // Removed from manifests/ + index.json: a restore of the pruned manifest now fails.
        assert!(snapshot_capture::restore(&store.to_string_lossy(), &m1, &scratch("gone").to_string_lossy())
            .is_err());
        let manifests_after = snapshot_capture::list_manifests(&store.to_string_lossy()).unwrap();
        assert!(!manifests_after.iter().any(|m| m.id == m1), "pruned manifest file is gone");

        // Store bytes dropped.
        let bytes_after = snapshot_capture::store_total_bytes(&store.to_string_lossy()).unwrap();
        assert!(bytes_after < bytes_before, "store footprint shrank after pruning the loser");

        // Survivors still restore byte-for-byte.
        let dest2 = scratch("restore-m2");
        snapshot_capture::restore(&store.to_string_lossy(), &m2, &dest2.to_string_lossy()).unwrap();
        assert_eq!(fs::read(dest2.join("a.txt")).unwrap(), b"v2");
        let dest3 = scratch("restore-m3");
        snapshot_capture::restore(&store.to_string_lossy(), &m3, &dest3.to_string_lossy()).unwrap();
        assert_eq!(fs::read(dest3.join("a.txt")).unwrap(), b"v3");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn apply_with_a_total_byte_cap_further_thins_survivors_oldest_first_but_never_to_zero() {
        let src = scratch("cap-src");
        let store = scratch("cap-store");
        // Distinct, sizeable content per capture so the byte cap actually bites.
        fs::write(src.join("a.txt"), vec![1u8; 1000]).unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), vec![2u8; 1000]).unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), vec![3u8; 1000]).unwrap();
        let m3 = capture_at(&src, &store, 3 * 3600);

        // A very generous hourly count keeps all three via GFS; the byte cap is the only pressure.
        let generous = RetentionPolicy { hourly: 10, daily: 0, weekly: 0, monthly: 0 };
        let total = snapshot_capture::store_total_bytes(&store.to_string_lossy()).unwrap();
        assert!(total >= 3000, "three distinct ~1000-byte blobs");

        // Cap tight enough to force dropping the oldest survivor but not everything.
        let cap = total - 1000;
        let result = apply(&store.to_string_lossy(), &generous, Some(cap)).unwrap();
        assert!(result.pruned.contains(&m1), "oldest survivor is evicted first for the byte cap");
        assert!(result.kept.contains(&m3), "newest survivor is protected");
        assert!(!result.kept.is_empty(), "byte cap must never prune every survivor");
        let _ = m2; // m2 may or may not survive depending on exact sizes; not asserted either way.

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn apply_never_prunes_the_last_survivor_even_under_a_tiny_cap() {
        let src = scratch("last-src");
        let store = scratch("last-store");
        fs::write(src.join("a.txt"), vec![9u8; 500]).unwrap();
        let only = capture_at(&src, &store, 3600);

        let pol = RetentionPolicy { hourly: 10, daily: 0, weekly: 0, monthly: 0 };
        let result = apply(&store.to_string_lossy(), &pol, Some(1)).unwrap();
        assert_eq!(result.kept, vec![only], "the single remaining snapshot is never wiped by the byte cap");
        assert!(result.pruned.is_empty());

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    #[test]
    fn apply_with_an_all_zero_policy_keeps_the_newest_survivor_not_a_full_wipe() {
        // Regression (CPE-1196 review): a policy that retains 0 in EVERY GFS tier made `thin()` return an
        // empty keep, and the GFS prune loop executed that as a full wipe — silent, irreversible loss of
        // the entire snapshot history (a zeroed schedule-rule retention was the real hazard). The floor
        // must keep the single newest snapshot.
        let src = scratch("zero-src");
        let store = scratch("zero-store");
        fs::write(src.join("a.txt"), b"v1").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"v2").unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);

        let zero = RetentionPolicy { hourly: 0, daily: 0, weekly: 0, monthly: 0 };
        let result = apply(&store.to_string_lossy(), &zero, None).unwrap();

        assert_eq!(result.kept, vec![m2.clone()], "the newest snapshot always survives an all-zero policy");
        assert_eq!(result.pruned, vec![m1.clone()]);
        // The survivor still restores byte-for-byte; the store is not empty.
        let dest = scratch("zero-restore");
        snapshot_capture::restore(&store.to_string_lossy(), &m2, &dest.to_string_lossy()).unwrap();
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"v2");
        assert!(!snapshot_capture::list_manifests(&store.to_string_lossy()).unwrap().is_empty());

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
        let _ = fs::remove_dir_all(&dest);
    }
}
