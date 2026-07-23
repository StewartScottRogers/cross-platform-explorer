//! Snapshot retention / thinning (CPE-944, epic CPE-735): a grandfather-father-son policy over a list of
//! local snapshots — keep the newest snapshot in each of the most-recent N hourly buckets, then daily,
//! weekly, and monthly — and report which snapshots to **keep** vs **prune**. Pure; the snapshot engine
//! takes and deletes the actual snapshots.

use std::collections::BTreeSet;

/// One taken snapshot: an opaque id + when it was taken (epoch seconds).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Snapshot {
    pub id: String,
    pub epoch_s: u64,
}

/// How many buckets to keep at each granularity. `0` disables a tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RetentionPolicy {
    pub hourly: usize,
    pub daily: usize,
    pub weekly: usize,
    pub monthly: usize,
}

impl Default for RetentionPolicy {
    /// A sensible time-machine-lite default: 24 hourly, 7 daily, 4 weekly, 12 monthly.
    fn default() -> Self {
        Self { hourly: 24, daily: 7, weekly: 4, monthly: 12 }
    }
}

/// The thinning decision: ids to keep and ids to prune (disjoint; together = every input snapshot).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RetentionResult {
    pub keep: Vec<String>,
    pub prune: Vec<String>,
}

const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;
const WEEK: u64 = 604_800;
const MONTH: u64 = 2_592_000; // 30 days

/// Decide which snapshots to keep under `policy`. For each tier, walking newest→oldest, keep the newest
/// snapshot in each of the most-recent `count` distinct time-buckets; a snapshot kept by *any* tier is
/// kept. Everything else is pruned. Deterministic; ids are returned newest-first (keep) / oldest-first
/// (prune) for a stable UI.
pub fn thin(snapshots: &[Snapshot], policy: &RetentionPolicy) -> RetentionResult {
    let mut sorted: Vec<&Snapshot> = snapshots.iter().collect();
    // Newest first; ties broken by id so the result is deterministic.
    sorted.sort_by(|a, b| b.epoch_s.cmp(&a.epoch_s).then_with(|| a.id.cmp(&b.id)));

    let mut keep: BTreeSet<String> = BTreeSet::new();
    for (secs, count) in [(HOUR, policy.hourly), (DAY, policy.daily), (WEEK, policy.weekly), (MONTH, policy.monthly)] {
        if count == 0 {
            continue;
        }
        let mut buckets: BTreeSet<u64> = BTreeSet::new();
        for s in &sorted {
            let bucket = s.epoch_s / secs;
            if buckets.contains(&bucket) {
                continue; // already kept the newest of this bucket
            }
            if buckets.len() >= count {
                break; // have enough recent buckets for this tier
            }
            buckets.insert(bucket);
            keep.insert(s.id.clone());
        }
    }

    let keep_ids: Vec<String> = sorted.iter().filter(|s| keep.contains(&s.id)).map(|s| s.id.clone()).collect();
    let prune_ids: Vec<String> =
        sorted.iter().rev().filter(|s| !keep.contains(&s.id)).map(|s| s.id.clone()).collect();
    RetentionResult { keep: keep_ids, prune: prune_ids }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(id: &str, epoch_s: u64) -> Snapshot {
        Snapshot { id: id.into(), epoch_s }
    }
    fn pol(h: usize, d: usize, w: usize, m: usize) -> RetentionPolicy {
        RetentionPolicy { hourly: h, daily: d, weekly: w, monthly: m }
    }

    #[test]
    fn empty_input_is_empty() {
        let r = thin(&[], &RetentionPolicy::default());
        assert!(r.keep.is_empty() && r.prune.is_empty());
    }

    #[test]
    fn hourly_keeps_newest_per_bucket_up_to_count() {
        // Three snapshots across two hours; keep 2 hourly buckets → keep the newest of each hour.
        let snaps = [
            snap("a", 3 * HOUR + 100), // hour 3
            snap("b", 3 * HOUR + 50),  // hour 3 (older, pruned)
            snap("c", 2 * HOUR + 10),  // hour 2
        ];
        let r = thin(&snaps, &pol(2, 0, 0, 0));
        assert_eq!(r.keep, vec!["a", "c"]);
        assert_eq!(r.prune, vec!["b"]);
    }

    #[test]
    fn count_limits_the_number_of_buckets() {
        let snaps = [snap("h5", 5 * HOUR), snap("h4", 4 * HOUR), snap("h3", 3 * HOUR)];
        let r = thin(&snaps, &pol(2, 0, 0, 0)); // only the 2 newest hour buckets
        assert_eq!(r.keep, vec!["h5", "h4"]);
        assert_eq!(r.prune, vec!["h3"]);
    }

    #[test]
    fn daily_tier_rescues_an_older_snapshot_the_hourly_tier_dropped() {
        // Two snapshots a day apart; hourly=1 keeps only the newest hour, but daily=2 keeps the older one too.
        let snaps = [snap("today", 10 * DAY + HOUR), snap("yesterday", 9 * DAY + HOUR)];
        let r = thin(&snaps, &pol(1, 2, 0, 0));
        assert!(r.keep.contains(&"today".to_string()) && r.keep.contains(&"yesterday".to_string()));
        assert!(r.prune.is_empty());
    }

    #[test]
    fn keep_and_prune_partition_every_input() {
        let snaps: Vec<Snapshot> = (0..20).map(|i| snap(&format!("s{i}"), i as u64 * HOUR)).collect();
        let r = thin(&snaps, &pol(3, 2, 0, 0));
        assert_eq!(r.keep.len() + r.prune.len(), 20);
        // No id appears in both.
        for k in &r.keep {
            assert!(!r.prune.contains(k));
        }
    }
}
