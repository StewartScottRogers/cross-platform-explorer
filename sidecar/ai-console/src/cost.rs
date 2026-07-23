//! Agent cost aggregation + budget status (CPE-913, epic CPE-731): the dashboard's compute core, built on
//! [`model_catalog::Pricing::estimate_cost`](crate::model_catalog::Pricing::estimate_cost). Pure functions
//! over already-tracked per-run costs — the UI renders the totals, the per-model breakdown, and the budget
//! gauge. Advisory display data, never billing.

use std::collections::BTreeMap;

/// A per-model USD breakdown + grand total.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CostRollup {
    pub total_usd: f64,
    /// Cost per model id, sorted by id (a `BTreeMap`) for a stable dashboard order.
    pub by_model: BTreeMap<String, f64>,
}

/// Aggregate `(model_id, cost_usd)` runs into a total + per-model breakdown.
pub fn rollup<'a, I>(runs: I) -> CostRollup
where
    I: IntoIterator<Item = (&'a str, f64)>,
{
    let mut out = CostRollup::default();
    for (model, cost) in runs {
        out.total_usd += cost;
        *out.by_model.entry(model.to_string()).or_insert(0.0) += cost;
    }
    out
}

/// How a running spend sits against a budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetLevel {
    /// Under the warning threshold.
    Ok,
    /// At or over 80% of the budget — surface a heads-up.
    Warn,
    /// At or over 100% of the budget — over budget.
    Over,
}

/// A budget gauge: how much is spent, what's left, the fraction used, and the alert level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetStatus {
    pub spent_usd: f64,
    pub budget_usd: f64,
    /// `budget - spent` (may be negative when over budget).
    pub remaining_usd: f64,
    /// `spent / budget` in `[0, ∞)`; `0.0` when the budget is 0 (avoids div-by-zero).
    pub fraction: f64,
    pub level: BudgetLevel,
}

/// Fraction of the budget at which to start warning (80%).
pub const BUDGET_WARN_FRACTION: f64 = 0.8;

/// Compute the [`BudgetStatus`] for `spent` against `budget`. A zero/negative budget means "no budget set":
/// fraction 0, level `Ok` (nothing to be over). Any positive spend at/over 80% warns, at/over 100% is over.
pub fn budget_status(spent_usd: f64, budget_usd: f64) -> BudgetStatus {
    let fraction = if budget_usd > 0.0 { spent_usd / budget_usd } else { 0.0 };
    let level = if budget_usd <= 0.0 {
        BudgetLevel::Ok
    } else if fraction >= 1.0 {
        BudgetLevel::Over
    } else if fraction >= BUDGET_WARN_FRACTION {
        BudgetLevel::Warn
    } else {
        BudgetLevel::Ok
    };
    BudgetStatus { spent_usd, budget_usd, remaining_usd: budget_usd - spent_usd, fraction, level }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollup_totals_and_breaks_down_by_model() {
        let runs = [
            ("anthropic/opus", 0.50),
            ("openai/gpt", 0.20),
            ("anthropic/opus", 0.25),
        ];
        let r = rollup(runs);
        assert!((r.total_usd - 0.95).abs() < 1e-9, "got {}", r.total_usd);
        assert!((r.by_model["anthropic/opus"] - 0.75).abs() < 1e-9);
        assert!((r.by_model["openai/gpt"] - 0.20).abs() < 1e-9);
        // BTreeMap → stable, sorted order.
        assert_eq!(r.by_model.keys().collect::<Vec<_>>(), vec!["anthropic/opus", "openai/gpt"]);
    }

    #[test]
    fn rollup_of_nothing_is_zero() {
        let r = rollup(std::iter::empty::<(&str, f64)>());
        assert_eq!(r.total_usd, 0.0);
        assert!(r.by_model.is_empty());
    }

    #[test]
    fn budget_status_levels_and_remaining() {
        // Half spent → Ok, half remaining.
        let s = budget_status(5.0, 10.0);
        assert_eq!(s.level, BudgetLevel::Ok);
        assert!((s.remaining_usd - 5.0).abs() < 1e-9);
        assert!((s.fraction - 0.5).abs() < 1e-9);

        // 85% → Warn.
        assert_eq!(budget_status(8.5, 10.0).level, BudgetLevel::Warn);
        // At/over 100% → Over, remaining goes negative.
        let over = budget_status(12.0, 10.0);
        assert_eq!(over.level, BudgetLevel::Over);
        assert!((over.remaining_usd - -2.0).abs() < 1e-9);

        // No budget set → Ok, fraction 0 (no div-by-zero).
        let none = budget_status(3.0, 0.0);
        assert_eq!(none.level, BudgetLevel::Ok);
        assert_eq!(none.fraction, 0.0);
    }
}
