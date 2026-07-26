//! Session metrics rollup (CPE-1071, epic CPE-731): the foundation row for the agent cost/resource
//! dashboard. [`fold_session`] sums a session's runs — tokens, cost, wall-clock, files, churn, edits —
//! into one [`SessionMetrics`] ledger row. Pure fold, built on
//! [`model_catalog::Pricing::estimate_cost`](crate::model_catalog::Pricing::estimate_cost); does not
//! modify `usage` or `model_catalog`. CPE-1072/1074 build on this module's `SessionMetrics`.

use crate::model_catalog::Pricing;

/// One run's raw counters, as recorded for a single agent invocation within a session.
pub struct RunRecord {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub wall_clock_ms: u64,
    pub files_touched: u64,
    pub churn_bytes: u64,
    pub edit_count: u64,
    pub model_id: String,
}

/// A session's runs folded into one multi-metric ledger row. `PartialEq`-not-`Eq` because
/// `cost_usd` is an `f64`. No serde/specta — mirrors `cost::CostRollup`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionMetrics {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
    pub wall_clock_ms: u64,
    pub files_touched: u64,
    pub churn_bytes: u64,
    pub edit_count: u64,
}

/// Fold a session's runs into one [`SessionMetrics`] row. Every counter sums via
/// [`u64::saturating_add`] so a pathological/adversarial run can't wrap or panic; `total_tokens` is the
/// saturating sum of the summed input + output. `cost_usd` is estimated from the *summed* tokens via
/// `pricing.estimate_cost`; when the pricing has no prompt/completion price at all, `estimate_cost`
/// returns `None` (nothing to estimate) and this folds it to `0.0` rather than propagating an `Option` —
/// an agent that reports tokens but no priced model still gets a ledger row, just a free one.
pub fn fold_session(runs: &[RunRecord], pricing: &Pricing) -> SessionMetrics {
    let mut m = SessionMetrics::default();
    for run in runs {
        m.input_tokens = m.input_tokens.saturating_add(run.input_tokens);
        m.output_tokens = m.output_tokens.saturating_add(run.output_tokens);
        m.wall_clock_ms = m.wall_clock_ms.saturating_add(run.wall_clock_ms);
        m.files_touched = m.files_touched.saturating_add(run.files_touched);
        m.churn_bytes = m.churn_bytes.saturating_add(run.churn_bytes);
        m.edit_count = m.edit_count.saturating_add(run.edit_count);
    }
    m.total_tokens = m.input_tokens.saturating_add(m.output_tokens);
    m.cost_usd = pricing.estimate_cost(m.input_tokens, m.output_tokens).unwrap_or(0.0);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(
        input_tokens: u64,
        output_tokens: u64,
        wall_clock_ms: u64,
        files_touched: u64,
        churn_bytes: u64,
        edit_count: u64,
    ) -> RunRecord {
        RunRecord {
            input_tokens,
            output_tokens,
            wall_clock_ms,
            files_touched,
            churn_bytes,
            edit_count,
            model_id: "anthropic/opus".to_string(),
        }
    }

    #[test]
    fn folds_n_runs_correctly() {
        let pricing = Pricing { prompt: Some(0.01), completion: Some(0.02) };
        let runs = [run(100, 50, 1_000, 2, 500, 3), run(200, 75, 2_000, 1, 300, 4), run(50, 25, 500, 3, 100, 1)];
        let m = fold_session(&runs, &pricing);

        assert_eq!(m.input_tokens, 350);
        assert_eq!(m.output_tokens, 150);
        assert_eq!(m.total_tokens, 500);
        assert_eq!(m.wall_clock_ms, 3_500);
        assert_eq!(m.files_touched, 6);
        assert_eq!(m.churn_bytes, 900);
        assert_eq!(m.edit_count, 8);

        let expected_cost = pricing.estimate_cost(350, 150).unwrap();
        assert!((m.cost_usd - expected_cost).abs() < 1e-9, "got {}", m.cost_usd);
    }

    #[test]
    fn two_u64_max_records_saturate_without_panic() {
        let pricing = Pricing::default();
        let runs = [
            run(u64::MAX, u64::MAX, u64::MAX, u64::MAX, u64::MAX, u64::MAX),
            run(u64::MAX, u64::MAX, u64::MAX, u64::MAX, u64::MAX, u64::MAX),
        ];
        let m = fold_session(&runs, &pricing);

        assert_eq!(m.input_tokens, u64::MAX);
        assert_eq!(m.output_tokens, u64::MAX);
        assert_eq!(m.total_tokens, u64::MAX);
        assert_eq!(m.wall_clock_ms, u64::MAX);
        assert_eq!(m.files_touched, u64::MAX);
        assert_eq!(m.churn_bytes, u64::MAX);
        assert_eq!(m.edit_count, u64::MAX);
    }

    #[test]
    fn empty_session_is_all_zero_default() {
        let pricing = Pricing { prompt: Some(0.01), completion: Some(0.02) };
        let m = fold_session(&[], &pricing);
        assert_eq!(m, SessionMetrics::default());
        assert_eq!(m.total_tokens, m.input_tokens + m.output_tokens);
        assert_eq!(m.cost_usd, 0.0);
    }

    #[test]
    fn total_tokens_always_equals_input_plus_output() {
        let pricing = Pricing::default();
        let runs = [run(10, 20, 0, 0, 0, 0), run(30, 5, 0, 0, 0, 0)];
        let m = fold_session(&runs, &pricing);
        assert_eq!(m.total_tokens, m.input_tokens + m.output_tokens);
    }

    #[test]
    fn cost_usd_matches_estimate_cost_over_summed_tokens() {
        let pricing = Pricing { prompt: Some(0.005), completion: Some(0.015) };
        let runs = [run(1_000, 500, 0, 0, 0, 0), run(2_000, 1_000, 0, 0, 0, 0)];
        let m = fold_session(&runs, &pricing);
        let expected = pricing.estimate_cost(3_000, 1_500).unwrap();
        assert!((m.cost_usd - expected).abs() < 1e-9);
    }

    #[test]
    fn no_pricing_at_all_folds_none_to_zero_cost() {
        // Pricing::default() has both prompt and completion as None -> estimate_cost returns None,
        // which fold_session documents as folding to 0.0 rather than propagating the Option.
        let pricing = Pricing::default();
        assert_eq!(pricing.estimate_cost(500, 500), None);

        let runs = [run(500, 500, 0, 0, 0, 0)];
        let m = fold_session(&runs, &pricing);
        assert_eq!(m.cost_usd, 0.0);
    }
}
