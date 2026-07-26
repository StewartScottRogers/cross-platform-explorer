//! Throughput time-series bucketing (CPE-1073, epic CPE-731): downsample timestamped
//! agent runs into fixed-width time buckets for a tokens/cost/files sparkline. A pure,
//! bounded, flat pass over the input — no recursion, no unbounded allocation.

/// One timestamped unit of agent work: tokens spent, cost incurred, files touched,
/// anchored at `start_ms` (only relative order/spacing matters here).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimedRun {
    pub start_ms: u64,
    pub tokens: u64,
    pub cost_usd: f64,
    pub files_touched: u64,
}

/// One bucket of the downsampled series: the window's start plus the summed tokens,
/// cost, and files-touched of every run that landed in it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Bucket {
    pub window_start_ms: u64,
    pub tokens: u64,
    pub cost_usd: f64,
    pub files_touched: u64,
}

/// Downsample `runs` into fixed-width buckets of `bucket_ms`, starting at `origin_ms`,
/// for a throughput sparkline.
///
/// A run's bucket index is `(start_ms - origin_ms) / bucket_ms` (the subtraction
/// saturates, so a run starting before `origin_ms` lands in bucket 0 rather than
/// underflowing), clamped to `max_buckets - 1`. That clamp means a run at or beyond the
/// tracked window — including the degenerate `start_ms == u64::MAX` case — always lands
/// in the FINAL bucket instead of panicking, wrapping, or being dropped.
///
/// `bucket_ms == 0` would make the division above divide-by-zero, so it (and
/// `max_buckets == 0`) short-circuits to an empty series instead. The returned vector is
/// sized to just past the highest bucket index actually used by `runs`, and never
/// exceeds `max_buckets` entries — a huge time span can't allocate unbounded memory.
/// Empty `runs` yields an empty series. Per-bucket integer sums use `saturating_add` so
/// pathological inputs (e.g. a `tokens` value near `u64::MAX`) can't wrap; `cost_usd` is
/// `f64` and simply accumulates (matching `cost::rollup`'s convention).
pub fn bucketize(runs: &[TimedRun], origin_ms: u64, bucket_ms: u64, max_buckets: usize) -> Vec<Bucket> {
    if runs.is_empty() || bucket_ms == 0 || max_buckets == 0 {
        return Vec::new();
    }

    let last_index = max_buckets - 1;
    let last_index_u64 = last_index as u64;

    // Precompute each run's clamped bucket index alongside it in one flat pass.
    let indexed: Vec<(usize, &TimedRun)> = runs
        .iter()
        .map(|run| {
            let offset = run.start_ms.saturating_sub(origin_ms);
            let idx = offset / bucket_ms; // bucket_ms > 0 guaranteed by the guard above.
            let idx = if idx > last_index_u64 { last_index } else { idx as usize };
            (idx, run)
        })
        .collect();

    // Size the output to just past the highest index actually used, capped at max_buckets.
    let bucket_count = indexed.iter().map(|(idx, _)| *idx).max().unwrap_or(0) + 1;
    let bucket_count = bucket_count.min(max_buckets);

    let mut buckets: Vec<Bucket> = (0..bucket_count)
        .map(|i| Bucket {
            window_start_ms: origin_ms.saturating_add((i as u64).saturating_mul(bucket_ms)),
            ..Bucket::default()
        })
        .collect();

    for (idx, run) in indexed {
        let bucket = &mut buckets[idx];
        bucket.tokens = bucket.tokens.saturating_add(run.tokens);
        bucket.cost_usd += run.cost_usd;
        bucket.files_touched = bucket.files_touched.saturating_add(run.files_touched);
    }

    buckets
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(start_ms: u64, tokens: u64, cost_usd: f64, files_touched: u64) -> TimedRun {
        TimedRun { start_ms, tokens, cost_usd, files_touched }
    }

    #[test]
    fn empty_runs_yield_empty_series() {
        assert_eq!(bucketize(&[], 0, 1000, 10), Vec::new());
    }

    #[test]
    fn zero_bucket_ms_never_divides_by_zero() {
        let runs = [run(0, 10, 1.0, 1), run(5_000, 20, 2.0, 2)];
        // Must not panic, and per the design, short-circuits to empty.
        assert_eq!(bucketize(&runs, 0, 0, 10), Vec::new());
    }

    #[test]
    fn zero_max_buckets_yields_empty_series() {
        let runs = [run(0, 10, 1.0, 1)];
        assert_eq!(bucketize(&runs, 0, 1000, 0), Vec::new());
    }

    #[test]
    fn runs_land_in_the_correct_bucket() {
        // Whole-number costs so the sums below are exact in f64 (no epsilon needed).
        let runs = [
            run(0, 1, 1.0, 1),      // bucket 0: [0, 1000)
            run(999, 2, 2.0, 1),    // bucket 0
            run(1_000, 3, 3.0, 1),  // bucket 1: [1000, 2000)
            run(2_500, 4, 4.0, 1),  // bucket 2: [2000, 3000)
        ];
        let buckets = bucketize(&runs, 0, 1000, 10);
        assert_eq!(buckets.len(), 3);
        assert_eq!(buckets[0], Bucket { window_start_ms: 0, tokens: 3, cost_usd: 3.0, files_touched: 2 });
        assert_eq!(buckets[1], Bucket { window_start_ms: 1000, tokens: 3, cost_usd: 3.0, files_touched: 1 });
        assert_eq!(buckets[2], Bucket { window_start_ms: 2000, tokens: 4, cost_usd: 4.0, files_touched: 1 });
    }

    #[test]
    fn origin_offsets_the_window_and_absorbs_earlier_runs() {
        // A run starting before origin_ms saturates to offset 0 -> bucket 0, not a panic.
        let runs = [run(500, 7, 7.0, 3), run(1_500, 8, 8.0, 4)];
        let buckets = bucketize(&runs, 1_000, 1_000, 10);
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0], Bucket { window_start_ms: 1_000, tokens: 15, cost_usd: 15.0, files_touched: 7 });
    }

    #[test]
    fn per_bucket_sums_saturate_instead_of_wrapping() {
        let runs = [run(0, u64::MAX, 0.0, u64::MAX), run(0, 5, 0.0, 5)];
        let buckets = bucketize(&runs, 0, 1000, 4);
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].tokens, u64::MAX);
        assert_eq!(buckets[0].files_touched, u64::MAX);
    }

    #[test]
    fn u64_max_start_clamps_into_the_final_bucket_without_panicking() {
        let runs = [run(0, 1, 0.1, 1), run(u64::MAX, 2, 0.2, 2)];
        let buckets = bucketize(&runs, 0, 1000, 5);
        assert_eq!(buckets.len(), 5);
        assert_eq!(buckets[4], Bucket { window_start_ms: 4000, tokens: 2, cost_usd: 0.2, files_touched: 2 });
        // The other (untouched) intermediate buckets stay at their default sums.
        assert_eq!(buckets[1], Bucket { window_start_ms: 1000, ..Bucket::default() });
    }

    #[test]
    fn output_length_never_exceeds_max_buckets() {
        // Runs spread over a huge span relative to bucket_ms/max_buckets.
        let runs: Vec<TimedRun> =
            (0..50).map(|i| run(i * 1_000_000, 1, 0.01, 1)).collect();
        let buckets = bucketize(&runs, 0, 1000, 8);
        assert!(buckets.len() <= 8, "got {} buckets", buckets.len());
        assert_eq!(buckets.len(), 8);
        // Every run past bucket 0's window (index 1_000_000/1000 = 1000, clamped) piles
        // into the final bucket rather than being dropped or panicking.
        assert_eq!(buckets[7].tokens, 49);
        assert_eq!(buckets[7].files_touched, 49);
    }

    #[test]
    fn result_is_deterministic() {
        let runs = [run(0, 1, 0.1, 1), run(1_500, 2, 0.2, 2), run(3_200, 3, 0.3, 3)];
        let a = bucketize(&runs, 0, 1000, 10);
        let b = bucketize(&runs, 0, 1000, 10);
        assert_eq!(a, b);
    }
}
