//! Spotlight frecency (CPE-952, epic CPE-704): rank recently + frequently used items for the overlay's
//! default (empty-query) view. A "frecency" score combines how OFTEN and how RECENTLY each item was used
//! — a recent-and-frequent item beats a stale-but-frequent or fresh-but-rare one. Pure + deterministic.

/// A tracked item's usage: how many times it's been opened and when it was last opened (epoch seconds).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Visit {
    pub path: String,
    pub count: u32,
    pub last_used_s: u64,
}

const HOUR: u64 = 3_600;
const DAY: u64 = 86_400;
const WEEK: u64 = 604_800;
const MONTH: u64 = 2_592_000;

/// A multiplier that decays with age — the classic Firefox-style frecency buckets.
pub fn recency_weight(age_s: u64) -> f64 {
    match age_s {
        a if a < HOUR => 4.0,
        a if a < DAY => 2.0,
        a if a < WEEK => 1.0,
        a if a < MONTH => 0.5,
        _ => 0.25,
    }
}

/// A visit's frecency score at `now_s`: `count × recency_weight(age)`. Higher is better.
pub fn frecency(v: &Visit, now_s: u64) -> f64 {
    v.count as f64 * recency_weight(now_s.saturating_sub(v.last_used_s))
}

/// Rank `visits` by frecency, best-first, returning up to `limit` paths. Ties break toward the more
/// recently used, then by path for determinism.
pub fn rank_frecent(visits: &[Visit], now_s: u64, limit: usize) -> Vec<String> {
    let mut scored: Vec<(f64, &Visit)> = visits.iter().map(|v| (frecency(v, now_s), v)).collect();
    scored.sort_by(|(sa, a), (sb, b)| {
        sb.partial_cmp(sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.last_used_s.cmp(&a.last_used_s))
            .then_with(|| a.path.cmp(&b.path))
    });
    scored.into_iter().take(limit).map(|(_, v)| v.path.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visit(path: &str, count: u32, last_used_s: u64) -> Visit {
        Visit { path: path.into(), count, last_used_s }
    }

    #[test]
    fn recency_weight_decays_by_bucket() {
        assert_eq!(recency_weight(0), 4.0); // this hour
        assert_eq!(recency_weight(2 * HOUR), 2.0); // today
        assert_eq!(recency_weight(3 * DAY), 1.0); // this week
        assert_eq!(recency_weight(2 * WEEK), 0.5); // this month
        assert_eq!(recency_weight(2 * MONTH), 0.25); // ancient
    }

    #[test]
    fn recent_and_frequent_beats_stale_or_rare() {
        let now = 100 * DAY;
        let hot = visit("/hot", 5, now - HOUR / 2); // 5 × 4.0 = 20
        let stale = visit("/stale", 20, now - 2 * MONTH); // 20 × 0.25 = 5
        let fresh_rare = visit("/rare", 1, now - HOUR / 2); // 1 × 4.0 = 4
        let out = rank_frecent(&[stale.clone(), fresh_rare.clone(), hot.clone()], now, 10);
        assert_eq!(out, vec!["/hot", "/stale", "/rare"]);
    }

    #[test]
    fn limit_caps_the_result() {
        let now = 10 * DAY;
        let visits: Vec<Visit> = (0..5).map(|i| visit(&format!("/v{i}"), i + 1, now - HOUR)).collect();
        let out = rank_frecent(&visits, now, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "/v4"); // highest count, same recency
    }

    #[test]
    fn empty_is_empty() {
        assert!(rank_frecent(&[], 0, 5).is_empty());
    }
}
