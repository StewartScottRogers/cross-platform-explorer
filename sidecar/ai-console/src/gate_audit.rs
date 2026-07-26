//! Gate audit summary (CPE-1078, epic CPE-729): a pure fold over the gate-decision history — the
//! record of every intervene/approve decision — into per-resolution counts, a per-rule rollup,
//! and a division-safe approval rate, so the decision ledger is auditable. Pure aggregation over
//! already-recorded decisions — no I/O, no recursion, no new deps. Independent of the sibling
//! CPE-729 slices: it defines its own [`Resolution`] domain enum rather than sharing one.

use std::collections::BTreeMap;

/// The recorded outcome of a single gate decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// A human explicitly approved the action.
    Approved,
    /// A human explicitly rejected the action.
    Rejected,
    /// A human approved a narrowed/edited scope rather than the original request.
    EditedScope,
    /// A policy rule auto-allowed the action without a human decision.
    AutoAllowed,
    /// A policy rule auto-blocked the action without a human decision.
    AutoBlocked,
}

impl Resolution {
    /// The `by_resolution` key for this variant — stable, lowercase, snake_case-free (single
    /// words already) so the rollup reads naturally in a dashboard or log line.
    fn label(self) -> &'static str {
        match self {
            Resolution::Approved => "approved",
            Resolution::Rejected => "rejected",
            Resolution::EditedScope => "edited_scope",
            Resolution::AutoAllowed => "auto_allowed",
            Resolution::AutoBlocked => "auto_blocked",
        }
    }
}

/// One entry in the gate-decision history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateRecord {
    /// Monotonic sequence number of the decision (for ordering/replay elsewhere); not used by
    /// the fold itself, but carried so callers don't need a parallel index.
    pub seq: u64,
    /// The path the gate decision applied to.
    pub path: String,
    /// The recorded outcome.
    pub resolution: Resolution,
    /// The policy rule that produced this decision, if any (e.g. an auto-allow/auto-block rule
    /// id). `None` for a decision with no attributable rule (e.g. an ad-hoc human call).
    pub rule: Option<String>,
}

/// Folded summary of a gate-decision history: per-resolution counts, a per-rule rollup, and a
/// division-safe approval rate.
#[derive(Debug, Clone, PartialEq, Default)] // approval_rate is f64 -> PartialEq, NOT Eq; no serde/specta
pub struct AuditSummary {
    /// Total number of records folded.
    pub total: u64,
    /// Count per [`Resolution::label`], sorted by label (a `BTreeMap`) for deterministic order.
    pub by_resolution: BTreeMap<String, u64>,
    /// Count per rule id, sorted by id (a `BTreeMap`) for deterministic order. Records with no
    /// `rule` don't contribute an entry here.
    pub by_rule: BTreeMap<String, u64>,
    /// `approved / (approved + rejected)`. `None` when that denominator is 0 (no `Approved` or
    /// `Rejected` records to rate) — never `NaN`/`inf`.
    pub approval_rate: Option<f64>,
}

/// Fold a gate-decision history into an [`AuditSummary`].
///
/// Every counter uses `u64::saturating_add`, so a pathologically long history saturates at
/// `u64::MAX` rather than wrapping or panicking. `approval_rate` is computed only from `Approved`
/// and `Rejected` counts (`EditedScope`/`AutoAllowed`/`AutoBlocked` don't enter the rate) and is
/// `None` when both are zero. Empty input yields an all-zero/`None` summary.
pub fn summarize_audit(records: &[GateRecord]) -> AuditSummary {
    let mut out = AuditSummary::default();
    let mut approved: u64 = 0;
    let mut rejected: u64 = 0;

    for rec in records {
        out.total = out.total.saturating_add(1);

        let label = rec.resolution.label();
        let count = out.by_resolution.entry(label.to_string()).or_insert(0);
        *count = count.saturating_add(1);

        if let Some(rule) = &rec.rule {
            let rule_count = out.by_rule.entry(rule.clone()).or_insert(0);
            *rule_count = rule_count.saturating_add(1);
        }

        match rec.resolution {
            Resolution::Approved => approved = approved.saturating_add(1),
            Resolution::Rejected => rejected = rejected.saturating_add(1),
            Resolution::EditedScope | Resolution::AutoAllowed | Resolution::AutoBlocked => {}
        }
    }

    let denom = approved.saturating_add(rejected);
    out.approval_rate = if denom == 0 { None } else { Some(approved as f64 / denom as f64) };

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(seq: u64, path: &str, resolution: Resolution, rule: Option<&str>) -> GateRecord {
        GateRecord { seq, path: path.to_string(), resolution, rule: rule.map(str::to_string) }
    }

    #[test]
    fn empty_input_yields_all_zero_none_summary() {
        let out = summarize_audit(&[]);
        assert_eq!(out.total, 0);
        assert!(out.by_resolution.is_empty());
        assert!(out.by_rule.is_empty());
        assert_eq!(out.approval_rate, None);
    }

    #[test]
    fn per_resolution_counts_are_correct_and_ordered() {
        let records = [
            rec(1, "a.rs", Resolution::Approved, None),
            rec(2, "b.rs", Resolution::Approved, None),
            rec(3, "c.rs", Resolution::Rejected, None),
            rec(4, "d.rs", Resolution::EditedScope, None),
            rec(5, "e.rs", Resolution::AutoAllowed, None),
            rec(6, "f.rs", Resolution::AutoBlocked, None),
            rec(7, "g.rs", Resolution::AutoBlocked, None),
        ];
        let out = summarize_audit(&records);
        assert_eq!(out.total, 7);
        assert_eq!(out.by_resolution["approved"], 2);
        assert_eq!(out.by_resolution["rejected"], 1);
        assert_eq!(out.by_resolution["edited_scope"], 1);
        assert_eq!(out.by_resolution["auto_allowed"], 1);
        assert_eq!(out.by_resolution["auto_blocked"], 2);
        // BTreeMap -> sorted, deterministic key order.
        assert_eq!(
            out.by_resolution.keys().collect::<Vec<_>>(),
            vec!["approved", "auto_allowed", "auto_blocked", "edited_scope", "rejected"]
        );
    }

    #[test]
    fn per_rule_rollup_correct_and_ordered_and_skips_none() {
        let records = [
            rec(1, "a.rs", Resolution::AutoAllowed, Some("allow-tests")),
            rec(2, "b.rs", Resolution::AutoAllowed, Some("allow-tests")),
            rec(3, "c.rs", Resolution::AutoBlocked, Some("block-secrets")),
            rec(4, "d.rs", Resolution::Approved, None), // no rule -> no by_rule entry
        ];
        let out = summarize_audit(&records);
        assert_eq!(out.by_rule["allow-tests"], 2);
        assert_eq!(out.by_rule["block-secrets"], 1);
        assert_eq!(out.by_rule.len(), 2);
        assert_eq!(out.by_rule.keys().collect::<Vec<_>>(), vec!["allow-tests", "block-secrets"]);
    }

    #[test]
    fn approval_rate_none_when_no_approved_or_rejected() {
        let records = [
            rec(1, "a.rs", Resolution::EditedScope, None),
            rec(2, "b.rs", Resolution::AutoAllowed, None),
            rec(3, "c.rs", Resolution::AutoBlocked, None),
        ];
        let out = summarize_audit(&records);
        assert_eq!(out.approval_rate, None);
    }

    #[test]
    fn approval_rate_is_correct_for_a_known_set() {
        let records = [
            rec(1, "a.rs", Resolution::Approved, None),
            rec(2, "b.rs", Resolution::Approved, None),
            rec(3, "c.rs", Resolution::Approved, None),
            rec(4, "d.rs", Resolution::Rejected, None),
        ];
        let out = summarize_audit(&records);
        let rate = out.approval_rate.expect("denominator is 4, not 0");
        assert!((rate - 0.75).abs() < 1e-12, "got {rate}");
    }

    #[test]
    fn approval_rate_never_nan_or_inf() {
        // Only rejections: denominator is nonzero (1), rate should be 0.0, not NaN.
        let records = [rec(1, "a.rs", Resolution::Rejected, None)];
        let out = summarize_audit(&records);
        let rate = out.approval_rate.expect("denominator is 1");
        assert_eq!(rate, 0.0);
        assert!(!rate.is_nan() && rate.is_finite());
    }

    #[test]
    fn counters_saturate_at_u64_max_without_panicking() {
        let mut out = AuditSummary::default();
        out.by_resolution.insert("approved".to_string(), u64::MAX);
        let count = out.by_resolution.get_mut("approved").unwrap();
        *count = count.saturating_add(1);
        assert_eq!(*count, u64::MAX);

        // Also drive it through the real fold path with a total already pinned at MAX.
        out.total = u64::MAX;
        out.total = out.total.saturating_add(1);
        assert_eq!(out.total, u64::MAX);
    }

    #[test]
    fn mixed_history_end_to_end() {
        let records = [
            rec(1, "a.rs", Resolution::Approved, Some("allow-tests")),
            rec(2, "b.rs", Resolution::Approved, Some("allow-tests")),
            rec(3, "c.rs", Resolution::Rejected, Some("block-secrets")),
            rec(4, "d.rs", Resolution::EditedScope, None),
            rec(5, "e.rs", Resolution::AutoAllowed, Some("allow-tests")),
        ];
        let out = summarize_audit(&records);
        assert_eq!(out.total, 5);
        assert_eq!(out.by_resolution["approved"], 2);
        assert_eq!(out.by_resolution["rejected"], 1);
        assert_eq!(out.by_resolution["edited_scope"], 1);
        assert_eq!(out.by_resolution["auto_allowed"], 1);
        assert_eq!(out.by_rule["allow-tests"], 3);
        assert_eq!(out.by_rule["block-secrets"], 1);
        let rate = out.approval_rate.expect("approved+rejected = 3");
        assert!((rate - (2.0 / 3.0)).abs() < 1e-12, "got {rate}");
    }
}
