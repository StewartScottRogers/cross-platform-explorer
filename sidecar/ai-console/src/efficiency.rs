//! Efficiency ratios + ranking (CPE-1074, epic CPE-731): division-safe cost-per-progress ratios
//! over a [`SessionMetrics`] row, plus a deterministic ranking helper over a batch of sessions.
//! Pure functions, no I/O — mirrors the derive/doc conventions of [`cost`](crate::cost). Reuses
//! [`session_metrics::SessionMetrics`](crate::session_metrics::SessionMetrics) as-is; does not
//! modify it.

use crate::session_metrics::SessionMetrics;

/// USD spent per file touched. `None` when `files_touched == 0` (nothing to divide by) — never
/// `inf`/`NaN`.
pub fn usd_per_file(m: &SessionMetrics) -> Option<f64> {
    if m.files_touched == 0 {
        None
    } else {
        Some(m.cost_usd / m.files_touched as f64)
    }
}

/// Tokens spent per file touched. `None` when `files_touched == 0`.
pub fn tokens_per_file(m: &SessionMetrics) -> Option<f64> {
    if m.files_touched == 0 {
        None
    } else {
        Some(m.total_tokens as f64 / m.files_touched as f64)
    }
}

/// Bytes of churn per 1,000 tokens spent. `None` when `total_tokens == 0`. `u64 -> f64` casts are
/// exact enough for a display ratio; even at `u64::MAX` the cast stays finite (no inf/NaN), it just
/// loses precision below 2^53 — acceptable for a ranking heuristic, not a billing figure.
pub fn churn_per_1k_tokens(m: &SessionMetrics) -> Option<f64> {
    if m.total_tokens == 0 {
        None
    } else {
        Some(m.churn_bytes as f64 / (m.total_tokens as f64 / 1000.0))
    }
}

/// Tokens processed per minute of wall-clock time. `None` when `wall_clock_ms == 0`.
pub fn tokens_per_minute(m: &SessionMetrics) -> Option<f64> {
    if m.wall_clock_ms == 0 {
        None
    } else {
        Some(m.total_tokens as f64 / (m.wall_clock_ms as f64 / 60000.0))
    }
}

/// Which efficiency ratio [`rank_by`] should rank on. No `f64` fields, so this derives `Eq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffMetric {
    UsdPerFile,
    TokensPerFile,
    ChurnPer1kTokens,
    TokensPerMinute,
}

impl EffMetric {
    fn ratio(self, m: &SessionMetrics) -> Option<f64> {
        match self {
            EffMetric::UsdPerFile => usd_per_file(m),
            EffMetric::TokensPerFile => tokens_per_file(m),
            EffMetric::ChurnPer1kTokens => churn_per_1k_tokens(m),
            EffMetric::TokensPerMinute => tokens_per_minute(m),
        }
    }
}

/// Rank `entries` (an `(id, SessionMetrics)` pair per agent/session) by the chosen [`EffMetric`],
/// returning `(id, ratio)` pairs.
///
/// **Order:** ascending by the raw ratio value — the lowest number sorts first. For the three
/// cost-shaped ratios (`UsdPerFile`, `TokensPerFile`, `ChurnPer1kTokens`) that reads naturally as
/// "cheapest first". `TokensPerMinute` is a throughput rate rather than a cost, so "ascending"
/// there means slowest-first; reverse the returned `Vec` at the call site if "fastest first" is
/// wanted for that metric. Keeping `rank_by` itself to one ascending order (rather than baking in
/// a per-metric "goodness" direction) keeps the ordering rule single and the tie-break simple.
///
/// **Ties:** entries with equal ratio values are ordered by `id` ascending (byte/lexicographic via
/// `str`'s `Ord`), so the result is fully deterministic regardless of input order.
///
/// **`None` placement:** an entry whose ratio is `None` (division-by-zero guard tripped) is
/// **always sorted last**, after every entry with a real ratio — including when *every* entry is
/// `None`, in which case the whole result falls back to id order. `None` entries are omitted from
/// the returned ratio (there is no `f64` to report), so callers that need to know *why* an entry
/// has no ranked ratio should re-derive it via [`EffMetric::ratio`]-equivalent calls
/// (`usd_per_file` etc.) — `rank_by` reports `0.0` as a placeholder for `None` entries in the
/// returned pair since `Vec<(String, f64)>` has no room for an `Option`; the stable last-place
/// position is what actually signals "no ratio", not the placeholder value.
pub fn rank_by(entries: &[(String, SessionMetrics)], metric: EffMetric) -> Vec<(String, f64)> {
    let mut scored: Vec<(&str, Option<f64>)> =
        entries.iter().map(|(id, m)| (id.as_str(), metric.ratio(m))).collect();

    scored.sort_by(|(id_a, ratio_a), (id_b, ratio_b)| {
        match (ratio_a, ratio_b) {
            (Some(a), Some(b)) => a
                .partial_cmp(b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| id_a.cmp(id_b)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => id_a.cmp(id_b),
        }
    });

    scored.into_iter().map(|(id, ratio)| (id.to_string(), ratio.unwrap_or(0.0))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(
        total_tokens: u64,
        cost_usd: f64,
        wall_clock_ms: u64,
        files_touched: u64,
        churn_bytes: u64,
    ) -> SessionMetrics {
        SessionMetrics {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens,
            cost_usd,
            wall_clock_ms,
            files_touched,
            churn_bytes,
            edit_count: 0,
        }
    }

    #[test]
    fn zero_files_gives_none_not_inf_or_nan() {
        let m = metrics(1_000, 5.0, 60_000, 0, 500);
        assert_eq!(usd_per_file(&m), None);
        assert_eq!(tokens_per_file(&m), None);
    }

    #[test]
    fn zero_tokens_gives_none_for_churn_ratio() {
        let m = metrics(0, 5.0, 60_000, 3, 500);
        assert_eq!(churn_per_1k_tokens(&m), None);
    }

    #[test]
    fn zero_wall_clock_gives_none_for_throughput() {
        let m = metrics(1_000, 5.0, 0, 3, 500);
        assert_eq!(tokens_per_minute(&m), None);
    }

    #[test]
    fn known_ratios_compute_exactly() {
        // 10 files, $5 spent, 2000 tokens, 500 churn bytes, 2 minutes wall clock.
        let m = metrics(2_000, 5.0, 120_000, 10, 500);
        assert!((usd_per_file(&m).unwrap() - 0.5).abs() < 1e-9);
        assert!((tokens_per_file(&m).unwrap() - 200.0).abs() < 1e-9);
        // 500 bytes / (2000 tokens / 1000) = 500 / 2 = 250.
        assert!((churn_per_1k_tokens(&m).unwrap() - 250.0).abs() < 1e-9);
        // 2000 tokens / (120_000ms / 60_000) = 2000 / 2 = 1000.
        assert!((tokens_per_minute(&m).unwrap() - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn ranking_orders_ascending_by_ratio() {
        let entries = vec![
            ("bravo".to_string(), metrics(1_000, 4.0, 60_000, 2, 0)), // usd/file = 2.0
            ("alpha".to_string(), metrics(1_000, 1.0, 60_000, 2, 0)), // usd/file = 0.5
            ("charlie".to_string(), metrics(1_000, 9.0, 60_000, 3, 0)), // usd/file = 3.0
        ];
        let ranked = rank_by(&entries, EffMetric::UsdPerFile);
        assert_eq!(ranked.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), vec![
            "alpha", "bravo", "charlie"
        ]);
        assert!((ranked[0].1 - 0.5).abs() < 1e-9);
        assert!((ranked[1].1 - 2.0).abs() < 1e-9);
        assert!((ranked[2].1 - 3.0).abs() < 1e-9);
    }

    #[test]
    fn ranking_is_deterministic_on_equal_value_ties() {
        // Same usd_per_file ratio (1.0) for all three -> tie-break must be id-ascending,
        // independent of input order, and stable across repeated calls.
        let entries = vec![
            ("zeta".to_string(), metrics(0, 2.0, 0, 2, 0)),
            ("alpha".to_string(), metrics(0, 2.0, 0, 2, 0)),
            ("mid".to_string(), metrics(0, 2.0, 0, 2, 0)),
        ];
        let ranked = rank_by(&entries, EffMetric::UsdPerFile);
        let ids: Vec<&str> = ranked.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "mid", "zeta"]);

        // Same input in a different order yields the same output order (determinism).
        let entries_reordered = vec![
            ("mid".to_string(), metrics(0, 2.0, 0, 2, 0)),
            ("zeta".to_string(), metrics(0, 2.0, 0, 2, 0)),
            ("alpha".to_string(), metrics(0, 2.0, 0, 2, 0)),
        ];
        let ranked_again = rank_by(&entries_reordered, EffMetric::UsdPerFile);
        assert_eq!(ranked_again.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ids);
    }

    #[test]
    fn none_ratio_entries_sort_last_after_all_real_ratios() {
        let entries = vec![
            ("no_files".to_string(), metrics(1_000, 5.0, 60_000, 0, 0)), // None
            ("has_files".to_string(), metrics(1_000, 5.0, 60_000, 5, 0)), // Some(1.0)
            ("also_no_files".to_string(), metrics(1_000, 9.0, 60_000, 0, 0)), // None
        ];
        let ranked = rank_by(&entries, EffMetric::UsdPerFile);
        let ids: Vec<&str> = ranked.iter().map(|(id, _)| id.as_str()).collect();
        // Real ratio first, then the two None entries, id-ordered between themselves.
        assert_eq!(ids, vec!["has_files", "also_no_files", "no_files"]);
    }

    #[test]
    fn all_none_falls_back_to_id_order() {
        let entries = vec![
            ("zeta".to_string(), metrics(1_000, 5.0, 60_000, 0, 0)),
            ("alpha".to_string(), metrics(1_000, 5.0, 60_000, 0, 0)),
        ];
        let ranked = rank_by(&entries, EffMetric::UsdPerFile);
        assert_eq!(ranked.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), vec![
            "alpha", "zeta"
        ]);
    }

    #[test]
    fn huge_u64_inputs_stay_finite_and_dont_produce_nonsense_rank() {
        let huge = metrics(u64::MAX, f64::MAX, u64::MAX, u64::MAX, u64::MAX);
        let small = metrics(1_000, 1.0, 60_000, 10, 500);
        let entries = vec![("huge".to_string(), huge.clone()), ("small".to_string(), small)];

        for metric in [
            EffMetric::UsdPerFile,
            EffMetric::TokensPerFile,
            EffMetric::ChurnPer1kTokens,
            EffMetric::TokensPerMinute,
        ] {
            let ranked = rank_by(&entries, metric);
            assert_eq!(ranked.len(), 2);
            for (_, ratio) in &ranked {
                assert!(ratio.is_finite(), "ratio must be finite for metric {metric:?}");
            }
        }

        // Direct ratio checks stay finite too (huge / huge is finite, e.g. ~1.0-ish for usd/file
        // scale since cost_usd is f64::MAX and files_touched is u64::MAX cast to f64).
        assert!(usd_per_file(&huge).unwrap().is_finite());
        assert!(tokens_per_file(&huge).unwrap().is_finite());
        assert!(churn_per_1k_tokens(&huge).unwrap().is_finite());
        assert!(tokens_per_minute(&huge).unwrap().is_finite());
    }

    #[test]
    fn empty_entries_ranks_to_empty() {
        let entries: Vec<(String, SessionMetrics)> = vec![];
        assert_eq!(rank_by(&entries, EffMetric::UsdPerFile), vec![]);
    }
}
