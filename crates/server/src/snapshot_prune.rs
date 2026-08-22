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
    // This loop is where the quadratic term lives: each `prune` re-scans the surviving manifests to
    // check nothing else still names the blobs it is about to free (CPE-1861). Fine for the scheduled
    // shape — one snapshot ageing out — and measurable on a bulk thin. Before optimising the retention
    // pass, read the cost note on `snapshot_capture::manifests_naming`: hoisting that scan out of the
    // per-manifest call is the fix, weakening it is not.
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

    // ---- CPE-1861 helpers ---------------------------------------------------------------------------

    /// The ids `list_manifests` currently hands the planner, sorted — i.e. exactly what
    /// [`crate::snapshot_retention::thin`] gets to decide about. Every CPE-1861 fixture asserts on this
    /// before and after its tamper, so "the tamper reached the planner" is proved rather than assumed.
    fn planner_view(store: &std::path::Path) -> Vec<String> {
        let mut v: Vec<String> = snapshot_capture::list_manifests(&store.to_string_lossy())
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect();
        v.sort();
        v
    }

    fn manifest_files(store: &std::path::Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(store.join("manifests"))
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Hand-edit `<file_id>.json`'s inner `id` field to `new_id`, and read it straight back so a tamper
    /// that silently failed to land can never be mistaken for a guard that worked.
    fn set_inner_id(store: &std::path::Path, file_id: &str, new_id: &str) {
        let path = store.join("manifests").join(format!("{file_id}.json"));
        let mut doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        doc["id"] = serde_json::json!(new_id);
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        let back: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["id"].as_str().unwrap(), new_id, "LIVE: the id tamper never landed on disk");
    }

    /// Copy `<file_id>.json` to `<new_name>.json`, optionally rewriting the copy's inner `id`. Returns
    /// the copy's path. Verified on disk before returning.
    fn plant_copy(
        store: &std::path::Path,
        file_id: &str,
        new_name: &str,
        inner_id: Option<&str>,
    ) -> std::path::PathBuf {
        let dir = store.join("manifests");
        let src = dir.join(format!("{file_id}.json"));
        let dst = dir.join(format!("{new_name}.json"));
        fs::copy(&src, &dst).unwrap();
        if let Some(id) = inner_id {
            let mut doc: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&dst).unwrap()).unwrap();
            doc["id"] = serde_json::json!(id);
            fs::write(&dst, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        }
        let back: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&dst).unwrap()).unwrap();
        assert!(back["files"].is_object(), "LIVE: the planted copy is not a parseable manifest");
        dst
    }

    /// Every id in `kept` must name a manifest that still restores. Before CPE-1861 a retention pass
    /// could report `kept: ["no-such-manifest"]` — a checkpoint it says it protected and that does not
    /// exist — and `kept: [m3, m2, m3]`, the same real checkpoint counted twice.
    fn assert_kept_ids_all_restore(store: &std::path::Path, kept: &[String], tag: &str) {
        let mut seen = std::collections::BTreeSet::new();
        for id in kept {
            assert!(seen.insert(id.clone()), "{tag}: kept names {id} twice: {kept:?}");
            let dest = scratch(&format!("kept-{tag}"));
            assert!(
                snapshot_capture::restore(&store.to_string_lossy(), id, &dest.to_string_lossy()).is_ok(),
                "{tag}: retention reported keeping {id}, which does not restore"
            );
            let _ = fs::remove_dir_all(&dest);
        }
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

    // ---- CPE-1861: a manifest's inner id vs the filename it is stored under -------------------------
    //
    // Every test below drives the real `apply` — the function `snapshot_run_due` calls after every
    // scheduled capture — and asserts its fixture is live in BOTH directions (the tamper landed on disk,
    // and the planner's own view changed because of it) before asserting any harm.

    /// Three captures, spread one hour apart, with the oldest one's inner `id` rewritten to name its
    /// newest sibling. Before: the phantom entry collapsed the whole decision — `Ok(kept: [m3, m2, m3],
    /// pruned: [])`, the same checkpoint reported kept twice and **nothing thinned at all**, with the
    /// tampered file immortal. After: the liar is simply not a checkpoint, the rest of the store is
    /// decided normally, and every id the result reports as kept is one that actually restores.
    #[test]
    fn cpe_1861_an_inner_id_naming_a_sibling_cannot_steer_or_stall_retention() {
        let src = scratch("steer-src");
        let store = scratch("steer-store");
        fs::write(src.join("a.txt"), b"v1").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"v2").unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), b"v3").unwrap();
        let m3 = capture_at(&src, &store, 3 * 3600);

        // FIXTURE LIVENESS 1 — the planner sees all three before the tamper.
        let before = planner_view(&store);
        assert_eq!(before.len(), 3, "fixture: {before:?}");
        assert!(before.contains(&m1));

        set_inner_id(&store, &m1, &m3);

        // FIXTURE LIVENESS 2 — the tamper reached the planner: its view changed, and the file is still
        // there (so a "no such file" could never be what this test is really measuring).
        let after = planner_view(&store);
        assert_ne!(before, after, "LIVE: the tamper never reached the planner");
        assert!(
            manifest_files(&store).contains(&format!("{m1}.json")),
            "LIVE: the tampered manifest file must still be on disk"
        );

        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap();
        assert_kept_ids_all_restore(&store, &result.kept, "steer");
        assert!(!after.contains(&m1), "the liar is not a checkpoint: {after:?}");
        assert!(result.kept.contains(&m2) && result.kept.contains(&m3));

        // The two honest survivors still restore byte-for-byte — the phantom stole nothing.
        for (id, want) in [(&m2, &b"v2"[..]), (&m3, &b"v3"[..])] {
            let dest = scratch("steer-restore");
            snapshot_capture::restore(&store.to_string_lossy(), id, &dest.to_string_lossy()).unwrap();
            assert_eq!(fs::read(dest.join("a.txt")).unwrap(), want);
            let _ = fs::remove_dir_all(&dest);
        }

        // Recorded cost, pinned so it is a decision and not a surprise: a self-inconsistent manifest is
        // never reclaimed. A leak — this module's chosen failure direction — not a delete.
        assert!(result.pruned.is_empty());
        assert!(manifest_files(&store).contains(&format!("{m1}.json")));

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// The worse of the two original shapes: an inner `id` naming nothing at all. Before, `apply`
    /// returned `Err(".../no-such-manifest.json: cannot find the file specified")` — and because
    /// `snapshot_run_due` retention-prunes after every scheduled capture, **no checkpoint in that store
    /// was ever thinned again**. After, the pass succeeds, thins normally, and keeps succeeding.
    #[test]
    fn cpe_1861_an_inner_id_naming_nothing_cannot_wedge_the_retention_pass() {
        let src = scratch("wedge-src");
        let store = scratch("wedge-store");
        let mut ids = Vec::new();
        for (n, body) in [(1u64, "v1"), (2, "v2"), (3, "v3"), (4, "v4")] {
            fs::write(src.join("a.txt"), body).unwrap();
            ids.push(capture_at(&src, &store, n * 3600));
        }
        let (m1, m2) = (ids[0].clone(), ids[1].clone());

        let before = planner_view(&store);
        assert_eq!(before.len(), 4, "fixture: {before:?}");

        set_inner_id(&store, &m1, "no-such-manifest");

        // FIXTURE LIVENESS — the tamper reached the enumerator the planner reads.
        let after = planner_view(&store);
        assert_ne!(before, after, "LIVE: the tamper never reached the planner");
        assert!(
            manifest_files(&store).contains(&format!("{m1}.json")),
            "LIVE: the tampered manifest file must still be on disk"
        );

        // hourly=2 keeps the two newest hour buckets; m2 is the oldest listed loser.
        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap_or_else(|e| {
            panic!("HARM: one manifest whose inner id names nothing killed the whole retention pass: {e}")
        });
        assert_eq!(result.pruned, vec![m2], "retention still thins the rest of the store");
        assert_kept_ids_all_restore(&store, &result.kept, "wedge");
        assert!(!after.contains(&"no-such-manifest".to_string()), "a phantom id is never listed: {after:?}");
        assert!(!after.contains(&m1), "{after:?}");

        // And it is not a one-shot recovery: the next scheduled pass succeeds too.
        assert!(apply(&store.to_string_lossy(), &all_pol(), None).is_ok(), "the pass stays alive");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// A crafted **filename**, which is where CPE-1847's fix relocated the wedge rather than removing
    /// it. Both shapes are covered: a copy whose inner id still names the original (caught by the
    /// agreement rule) and a self-*consistent* one whose inner id is the crafted name too (caught only
    /// by `validate_manifest_id`, which is why that second condition exists).
    #[test]
    fn cpe_1861_a_crafted_manifest_filename_cannot_wedge_the_retention_pass() {
        let names: &[&str] = if cfg!(unix) { &["a..b", "a:b", "a\\b"] } else { &["a..b"] };
        for name in names {
            for self_consistent in [false, true] {
                let src = scratch("name-src");
                let store = scratch("name-store");
                fs::write(src.join("a.txt"), b"v1").unwrap();
                let m1 = capture_at(&src, &store, 3600);
                fs::write(src.join("a.txt"), b"v2").unwrap();
                let m2 = capture_at(&src, &store, 2 * 3600);
                fs::write(src.join("a.txt"), b"v3").unwrap();
                let _m3 = capture_at(&src, &store, 3 * 3600);

                let before = planner_view(&store);
                assert_eq!(before.len(), 3, "fixture: {before:?}");
                let planted =
                    plant_copy(&store, &m1, name, if self_consistent { Some(name) } else { None });

                // FIXTURE LIVENESS — the file really is there under the crafted name, and the planner
                // still refuses to list it (so a filesystem that rejected the name outright, which is
                // what would silently make this test inert on Windows, cannot pass for a guard).
                assert!(planted.exists(), "LIVE: {name}.json is not on disk");
                assert!(
                    manifest_files(&store).contains(&format!("{name}.json")),
                    "LIVE: {name}.json is not in manifests/"
                );
                let after = planner_view(&store);
                let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap_or_else(|e| {
                    panic!("HARM: planting {name}.json killed the whole retention pass: {e}")
                });
                assert_eq!(result.pruned, vec![m1.clone()], "retention still thins normally ({name})");
                assert!(result.kept.contains(&m2));
                assert_kept_ids_all_restore(&store, &result.kept, "name");
                assert_eq!(after, before, "a crafted filename is never a checkpoint: {after:?}");

                let _ = fs::remove_dir_all(&src);
                let _ = fs::remove_dir_all(&store);
            }
        }
    }

    /// CPE-1847 added the `file_count` self-consistency refusal and recorded, next to it, that one
    /// tampered manifest therefore wedged the whole retention pass — the same permanent stall as an
    /// unresolvable id, since `apply` propagates `prune`'s error with `?`. Closed from the enumeration
    /// end: `list_manifests` applies the *same* predicate as a skip, so the pass never names it. The
    /// refusal itself is unchanged and still fires on every read route.
    #[test]
    fn cpe_1861_a_manifest_contradicting_its_own_count_no_longer_wedges_the_pass() {
        let src = scratch("count-src");
        let store = scratch("count-store");
        fs::write(src.join("a.txt"), b"v1").unwrap();
        fs::write(src.join("b.txt"), b"b").unwrap();
        let m1 = capture_at(&src, &store, 3600);
        fs::write(src.join("a.txt"), b"v2").unwrap();
        let m2 = capture_at(&src, &store, 2 * 3600);
        fs::write(src.join("a.txt"), b"v3").unwrap();
        let _m3 = capture_at(&src, &store, 3 * 3600);

        let before = planner_view(&store);
        assert_eq!(before.len(), 3, "fixture: {before:?}");

        // Remove one entry from the map and leave the count behind — the tamper the count exists to
        // catch.
        let path = store.join("manifests").join(format!("{m1}.json"));
        let mut doc: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        doc["files"].as_object_mut().unwrap().remove("b.txt").unwrap();
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

        // FIXTURE LIVENESS — the refusal really fires for this file (so the skip below is skipping a
        // manifest that would genuinely have errored), and the planner's view changed.
        assert!(
            snapshot_capture::manifest_snapshot(&store.to_string_lossy(), &m1)
                .unwrap_err()
                .contains("contradicts itself"),
            "LIVE: the count tamper did not make this manifest unloadable"
        );
        // Deliberately NOT "the planner's view changed" here, the way the id fixtures above prove their
        // tamper reached the enumerator: this tamper's only visible effect on that view *is* the guard,
        // so asserting it before the harm would make a removed guard red on a proxy instead of on the
        // stall itself. The liveness above is the independent one — the file really is unloadable now.
        let after = planner_view(&store);
        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap_or_else(|e| {
            panic!("HARM: a manifest contradicting its own count killed the whole retention pass: {e}")
        });
        assert!(result.kept.contains(&m2));
        assert_kept_ids_all_restore(&store, &result.kept, "count");
        assert!(!after.contains(&m1), "a self-contradicting manifest is not a checkpoint: {after:?}");
        assert_eq!(after.len(), 2);

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }

    /// **The acceptance gate.** A second copy of a manifest file — Explorer copy/paste, a cloud-sync
    /// conflict copy, a backup script, a partial restore-from-backup — must never cost the surviving
    /// checkpoint its content. CPE-1847's reverted fix turned this ordinary event into silent
    /// unattended data loss: two copies got two ids, retention pruned one, `release` dropped the
    /// **shared** blob refcounts to zero, and the manifest reported as `kept` could restore nothing.
    ///
    /// Two independent guards now stand between that fixture and the harm, and this test asserts the
    /// outcome rather than either mechanism: the copy is not a checkpoint (`list_manifests`), and even
    /// if it were, pruning it could not free a blob the survivor still names (`prune`).
    #[test]
    fn cpe_1861_a_duplicated_manifest_file_never_costs_the_survivor_its_content() {
        let src = scratch("dup-src");
        let store = scratch("dup-store");
        fs::write(src.join("a.txt"), b"the only copy of this content").unwrap();
        let id = capture_at(&src, &store, 3600);

        let before = planner_view(&store);
        assert_eq!(before, vec![id.clone()], "fixture: {before:?}");
        plant_copy(&store, &id, &format!("{id}-backup"), None);

        // FIXTURE LIVENESS — the copy is on disk, and it genuinely names the same blob the original
        // does, which is the whole reason pruning it was able to destroy the original.
        assert_eq!(manifest_files(&store).len(), 2, "LIVE: the copy is not in manifests/");
        let copy_doc: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(store.join("manifests").join(format!("{id}-backup.json"))).unwrap(),
        )
        .unwrap();
        let hash = copy_doc["files"]["a.txt"]["hash"].as_str().unwrap().to_string();
        assert!(store.join("blobs").join(&hash).exists(), "LIVE: the shared blob is missing");

        // FIXTURE LIVENESS, and the measurement behind the whole design choice: **the refcount cannot
        // answer "does another manifest still name this blob".** `index.json` says one reference while
        // two files on disk name it — a copy adds a namer without ever going through a capture, so the
        // counter and the manifest set disagree by construction.
        let index: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(store.join("index.json")).unwrap()).unwrap();
        assert_eq!(
            index["blobs"][&hash]["refs"].as_u64(),
            Some(1),
            "LIVE: the refcount/namer drift this ticket turns on is not present in the fixture"
        );

        let result = apply(&store.to_string_lossy(), &all_pol(), None).unwrap();

        // THE GATE, asserted on content before anything else: whatever retention says it kept, restores.
        assert!(!result.kept.is_empty());
        for kept in &result.kept {
            let dest = scratch("dup-restore");
            snapshot_capture::restore(&store.to_string_lossy(), kept, &dest.to_string_lossy())
                .unwrap_or_else(|e| panic!("HARM: retention kept {kept} and it cannot restore: {e}"));
            assert_eq!(
                fs::read(dest.join("a.txt")).unwrap(),
                b"the only copy of this content",
                "HARM: the kept checkpoint's tree did not come back"
            );
            let _ = fs::remove_dir_all(&dest);
        }
        assert!(store.join("blobs").join(&hash).exists(), "HARM: the shared blob was deleted");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&store);
    }
}
