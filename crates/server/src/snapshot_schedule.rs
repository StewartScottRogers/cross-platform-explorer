//! Snapshot schedule rules store + pure `due()` policy (CPE-1198, epic CPE-735): the headless core of the
//! periodic-snapshot scheduler, decoupled from any background timer (the timer + UI land in CPE-1199).
//!
//! The rule store mirrors [`crate::column_config`]'s pattern exactly: a folder-path-keyed JSON catalog in
//! the app config dir, reached through [`ServerCtx`], tolerant read (an absent or corrupt file yields an
//! empty catalog rather than erroring). [`due`] is pure — given the rules, the current time, and each
//! root's last-run time, it decides which roots need a fresh capture; `now`/`last_run_times` are always
//! caller-injected so this is fully deterministic and unit-testable without a real clock.
//! [`snapshot_run_due`] is the one impure entry point: it captures every due root (via
//! [`crate::checkpoint_store::checkpoint_create`]) and retention-prunes it (via
//! [`crate::checkpoint_store::checkpoint_prune_apply`], CPE-1196). No timer and no UI live here — this is
//! a single pass a caller (a future timer, or a "run now" button) invokes.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::checkpoint_store;
use crate::ctx::ServerCtx;
use crate::snapshot_prune::RetentionApplyResult;
use crate::snapshot_retention::RetentionPolicy;

/// One folder's periodic-snapshot rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ScheduleRule {
    /// The watched root this rule captures.
    pub root: String,
    /// Minimum seconds that must elapse since the last run before `root` is due again.
    pub interval_s: u64,
    /// The retention policy [`snapshot_run_due`] applies to `root` after each scheduled capture.
    pub retention: RetentionPolicy,
    /// A disabled rule is never returned by [`due`].
    pub enabled: bool,
}

/// The stored rule catalog: root path → [`ScheduleRule`]. `BTreeMap` for a stable, diff-friendly on-disk
/// order (mirrors [`crate::column_config::Catalog`]).
pub type Catalog = BTreeMap<String, ScheduleRule>;

fn rules_path(dir: &Path) -> PathBuf {
    dir.join("snapshot_schedule.json")
}

/// Read the catalog from `snapshot_schedule.json` in `dir`; an absent or corrupt file yields an empty
/// catalog.
fn read_catalog_from(dir: &Path) -> Catalog {
    fs::read_to_string(rules_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the catalog to `snapshot_schedule.json` in `dir`, creating `dir` if needed.
fn write_catalog_to(dir: &Path, catalog: &Catalog) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(catalog).map_err(|e| e.to_string())?;
    fs::write(rules_path(dir), json.as_bytes()).map_err(|e| e.to_string())
}

/// Every stored rule, root-path-sorted (the `BTreeMap`'s natural order).
pub fn list_rules(ctx: &dyn ServerCtx) -> Result<Vec<ScheduleRule>, String> {
    Ok(read_catalog_from(&ctx.app_config_dir()?).into_values().collect())
}

/// The stored rule for `root`, if any.
pub fn get_rule(ctx: &dyn ServerCtx, root: &str) -> Result<Option<ScheduleRule>, String> {
    Ok(read_catalog_from(&ctx.app_config_dir()?).get(root).cloned())
}

/// Save (insert or replace by `root`) a rule and persist. Preserves every other root's entry.
pub fn set_rule(ctx: &dyn ServerCtx, rule: ScheduleRule) -> Result<(), String> {
    let dir = ctx.app_config_dir()?;
    let mut catalog = read_catalog_from(&dir);
    catalog.insert(rule.root.clone(), rule);
    write_catalog_to(&dir, &catalog)
}

/// Remove `root`'s rule, if any, and persist. Leaves every other root's entry intact.
pub fn remove_rule(ctx: &dyn ServerCtx, root: &str) -> Result<(), String> {
    let dir = ctx.app_config_dir()?;
    let mut catalog = read_catalog_from(&dir);
    catalog.remove(root);
    write_catalog_to(&dir, &catalog)
}

/// Pure decision: which of `rules`' roots are due for a fresh capture at `now` (epoch seconds), given each
/// root's last successful run in `last_run_times` (root → epoch seconds). A disabled rule is never due. A
/// root absent from `last_run_times` has never run and is due immediately. Otherwise a root is due once
/// `now - last_run >= interval_s`. Deterministic — `now` is always caller-injected, never read from the
/// wall clock — so this needs no sleeping to test. Order follows `rules`' order (the catalog's
/// root-sorted order, when called via [`list_rules`]).
pub fn due(rules: &[ScheduleRule], now: u64, last_run_times: &BTreeMap<String, u64>) -> Vec<String> {
    rules
        .iter()
        .filter(|r| r.enabled)
        .filter(|r| match last_run_times.get(&r.root) {
            Some(&last) => now.saturating_sub(last) >= r.interval_s,
            None => true, // never run → due immediately
        })
        .map(|r| r.root.clone())
        .collect()
}

/// One due root's capture + retention outcome, as reported by [`snapshot_run_due`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RunDueOutcome {
    pub root: String,
    /// The manifest id this scheduled capture just wrote.
    pub manifest_id: String,
    /// CPE-1196's retention-prune result, applied to `root` right after the capture.
    pub retained: RetentionApplyResult,
}

/// Capture every root [`due`] returns (via [`checkpoint_store::checkpoint_create`], labelled
/// `"scheduled"`) then retention-prune it to its own rule's policy (via
/// [`checkpoint_store::checkpoint_prune_apply`], CPE-1196). `now`/`last_run_times` are injected, exactly
/// like [`due`] — a future timer (CPE-1199) supplies wall-clock `now` and its own persisted last-run
/// bookkeeping; this function itself never touches a clock or writes last-run state. No timer, no UI.
///
/// A root whose capture fails (unreadable, vanished) is skipped rather than aborting the rest of the
/// batch — mirrors this codebase's skip-on-error guardrail (`list_dir`, `snapshot_capture::scan_dir`, …).
pub fn snapshot_run_due(
    ctx: &dyn ServerCtx,
    now: u64,
    last_run_times: &BTreeMap<String, u64>,
) -> Result<Vec<RunDueOutcome>, String> {
    let rules = list_rules(ctx)?;
    let due_roots = due(&rules, now, last_run_times);
    let by_root: BTreeMap<&str, &ScheduleRule> = rules.iter().map(|r| (r.root.as_str(), r)).collect();

    let mut out = Vec::new();
    for root in due_roots {
        let Some(rule) = by_root.get(root.as_str()) else { continue };
        let Ok(created) = checkpoint_store::checkpoint_create(ctx, &root, "scheduled") else { continue };
        let retained =
            checkpoint_store::checkpoint_prune_apply(ctx, &root, &rule.retention, None)?;
        out.push(RunDueOutcome { root, manifest_id: created.checkpoint.manifest_id, retained });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::HeadlessCtx;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-snapsched-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn rule(root: &str, interval_s: u64, enabled: bool) -> ScheduleRule {
        ScheduleRule {
            root: root.to_string(),
            interval_s,
            retention: RetentionPolicy::default(),
            enabled,
        }
    }

    // ---- rule CRUD ---------------------------------------------------------------------------------

    #[test]
    fn set_then_get_round_trips_and_list_reflects_it() {
        let base = scratch("crud-roundtrip");
        let ctx = HeadlessCtx::new(&base);
        assert!(list_rules(&ctx).unwrap().is_empty());

        set_rule(&ctx, rule("/photos", 3600, true)).unwrap();
        assert_eq!(get_rule(&ctx, "/photos").unwrap(), Some(rule("/photos", 3600, true)));
        assert_eq!(list_rules(&ctx).unwrap(), vec![rule("/photos", 3600, true)]);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn set_replaces_same_root_rule_and_remove_deletes_only_that_one() {
        let base = scratch("crud-replace");
        let ctx = HeadlessCtx::new(&base);
        set_rule(&ctx, rule("/a", 100, true)).unwrap();
        set_rule(&ctx, rule("/b", 200, true)).unwrap();
        set_rule(&ctx, rule("/a", 999, false)).unwrap(); // replace /a

        assert_eq!(get_rule(&ctx, "/a").unwrap(), Some(rule("/a", 999, false)));
        assert_eq!(get_rule(&ctx, "/b").unwrap(), Some(rule("/b", 200, true)));
        assert_eq!(list_rules(&ctx).unwrap().len(), 2);

        remove_rule(&ctx, "/a").unwrap();
        assert!(get_rule(&ctx, "/a").unwrap().is_none());
        assert!(get_rule(&ctx, "/b").unwrap().is_some());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_and_corrupt_catalog_are_tolerated_as_empty() {
        let base = scratch("crud-corrupt");
        let ctx = HeadlessCtx::new(&base);
        assert!(list_rules(&ctx).unwrap().is_empty(), "no file yet");

        let dir = ctx.app_config_dir().unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(rules_path(&dir), b"{ not valid json").unwrap();
        assert!(list_rules(&ctx).unwrap().is_empty(), "corrupt file degrades to empty, not an error");

        // A subsequent save overwrites the corruption cleanly.
        set_rule(&ctx, rule("/fresh", 60, true)).unwrap();
        assert_eq!(list_rules(&ctx).unwrap().len(), 1);
        let _ = fs::remove_dir_all(&base);
    }

    // ---- due() --------------------------------------------------------------------------------------

    #[test]
    fn due_excludes_disabled_rules() {
        let rules = vec![rule("/a", 60, true), rule("/b", 60, false)];
        let got = due(&rules, 1000, &BTreeMap::new());
        assert_eq!(got, vec!["/a".to_string()], "disabled /b is never due regardless of timing");
    }

    #[test]
    fn due_returns_a_root_with_no_recorded_last_run_immediately() {
        let rules = vec![rule("/never-run", 3600, true)];
        let got = due(&rules, 5, &BTreeMap::new());
        assert_eq!(got, vec!["/never-run".to_string()]);
    }

    #[test]
    fn due_respects_the_interval_past_and_not_yet() {
        let rules = vec![rule("/a", 100, true), rule("/b", 100, true)];
        let mut last = BTreeMap::new();
        last.insert("/a".to_string(), 500u64); // 500 + 100 = 600, now=600 → due (>=)
        last.insert("/b".to_string(), 550u64); // 550 + 100 = 650, now=600 → not yet due

        let got = due(&rules, 600, &last);
        assert_eq!(got, vec!["/a".to_string()], "only /a has crossed its interval");
    }

    #[test]
    fn due_is_deterministic_over_an_injected_now() {
        let rules = vec![rule("/a", 10, true)];
        let mut last = BTreeMap::new();
        last.insert("/a".to_string(), 100u64);
        assert!(due(&rules, 109, &last).is_empty(), "1 second short of the interval");
        assert_eq!(due(&rules, 110, &last), vec!["/a".to_string()], "exactly at the interval");
        assert_eq!(due(&rules, 200, &last), vec!["/a".to_string()], "well past the interval");
    }

    // ---- snapshot_run_due -----------------------------------------------------------------------------

    #[test]
    fn run_due_captures_and_prunes_only_due_enabled_roots() {
        let app = scratch("rundue-app");
        let ctx = HeadlessCtx::new(&app);
        let root_a = scratch("rundue-root-a");
        let root_b = scratch("rundue-root-b");
        fs::write(root_a.join("f.txt"), b"a content").unwrap();
        fs::write(root_b.join("f.txt"), b"b content").unwrap();
        let root_a_s = root_a.to_string_lossy().to_string();
        let root_b_s = root_b.to_string_lossy().to_string();

        set_rule(&ctx, rule(&root_a_s, 60, true)).unwrap(); // due (never run)
        set_rule(&ctx, rule(&root_b_s, 60, false)).unwrap(); // disabled → never due

        let outcomes = snapshot_run_due(&ctx, 1000, &BTreeMap::new()).unwrap();
        assert_eq!(outcomes.len(), 1, "only the enabled, due root is captured");
        assert_eq!(outcomes[0].root, root_a_s);
        assert!(!outcomes[0].manifest_id.is_empty());

        // The capture actually landed in that root's checkpoint store.
        let list = checkpoint_store::checkpoint_list(&ctx, &root_a_s).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].manifest_id, outcomes[0].manifest_id);

        let _ = fs::remove_dir_all(&root_a);
        let _ = fs::remove_dir_all(&root_b);
        let _ = fs::remove_dir_all(&app);
    }

    #[test]
    fn run_due_is_deterministic_with_an_injected_now_and_last_run_map() {
        let app = scratch("rundue-deterministic");
        let ctx = HeadlessCtx::new(&app);
        let root = scratch("rundue-det-root");
        fs::write(root.join("f.txt"), b"content").unwrap();
        let root_s = root.to_string_lossy().to_string();
        set_rule(&ctx, rule(&root_s, 3600, true)).unwrap();

        let mut last_run = BTreeMap::new();
        last_run.insert(root_s.clone(), 1_000u64);

        // Not yet due at now=2000 (interval 3600s).
        assert!(snapshot_run_due(&ctx, 2_000, &last_run).unwrap().is_empty());
        assert!(checkpoint_store::checkpoint_list(&ctx, &root_s).unwrap().is_empty());

        // Due at now=4601 (>= 1000 + 3600).
        let outcomes = snapshot_run_due(&ctx, 4_601, &last_run).unwrap();
        assert_eq!(outcomes.len(), 1);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }

    #[test]
    fn run_due_applies_retention_pruning_to_each_captured_root() {
        let app = scratch("rundue-retain-app");
        let ctx = HeadlessCtx::new(&app);
        let root = scratch("rundue-retain-root");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("f.txt"), b"v1").unwrap();

        // A tight retention (hourly=1) so a rule root that already has an older checkpoint gets it pruned
        // once the scheduled capture lands (both fall in "now"'s hour bucket in a fast test run, so the
        // older one is the GFS loser).
        let mut r = rule(&root_s, 1, true);
        r.retention = RetentionPolicy { hourly: 1, daily: 0, weekly: 0, monthly: 0 };
        set_rule(&ctx, r).unwrap();

        let first = checkpoint_store::checkpoint_create(&ctx, &root_s, "manual").unwrap();
        fs::write(root.join("f.txt"), b"v2").unwrap();

        let outcomes = snapshot_run_due(&ctx, 10_000, &BTreeMap::new()).unwrap();
        assert_eq!(outcomes.len(), 1);
        // Both the manual and the scheduled capture are accounted for one way or the other (kept or
        // pruned) — `thin()`'s tie-break at second resolution doesn't guarantee which of two
        // same-bucket manifests survives, only that hourly=1 collapses the pair down to exactly one.
        let retained = &outcomes[0].retained;
        assert_eq!(retained.kept.len() + retained.pruned.len(), 2, "both manifests are accounted for");
        assert_eq!(retained.kept.len(), 1, "hourly=1 over a same-bucket pair keeps exactly one");
        assert!(
            retained.kept.contains(&first.checkpoint.manifest_id)
                || retained.kept.contains(&outcomes[0].manifest_id),
            "the survivor is one of the two manifests actually captured"
        );

        // Retention actually ran (not a no-op): whichever manifest it pruned is gone from disk — it can
        // no longer be previewed/reverted to — while the survivor still can be.
        let pruned_id = &retained.pruned[0];
        let kept_id = &retained.kept[0];
        assert!(checkpoint_store::checkpoint_preview_revert(&ctx, &root_s, pruned_id, None).is_err());
        assert!(checkpoint_store::checkpoint_preview_revert(&ctx, &root_s, kept_id, None).is_ok());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&app);
    }
}
