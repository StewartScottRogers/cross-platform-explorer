//! Fleet aggregation (CPE-1072, epic CPE-731): rolls many sessions' [`SessionMetrics`] up into
//! fleet totals, per-agent, and per-model breakdowns, plus division-safe averages. Pure fold, built
//! on [`session_metrics::SessionMetrics`](crate::session_metrics::SessionMetrics); does not modify
//! that module. Mirrors `cost::rollup`'s `BTreeMap` shape for deterministic, sorted dashboard order.

use std::collections::BTreeMap;

use crate::session_metrics::SessionMetrics;

/// Division-safe per-session averages across the fleet. All `f64` — `0.0` across the board when
/// there are no sessions (never divides by zero).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionAverages {
    pub avg_cost_usd: f64,
    pub avg_tokens: f64,
    pub avg_wall_clock_ms: f64,
    pub avg_files_touched: f64,
    pub avg_churn_bytes: f64,
}

/// Fleet-wide rollup: grand totals, per-model and per-agent breakdowns (both `BTreeMap` → sorted,
/// deterministic order), the session count, and the division-safe averages.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FleetRollup {
    pub totals: SessionMetrics,
    /// `SessionMetrics` summed per model id, sorted by id for a stable dashboard order.
    pub by_model: BTreeMap<String, SessionMetrics>,
    /// `SessionMetrics` summed per agent id, sorted by id for a stable dashboard order.
    pub by_agent: BTreeMap<String, SessionMetrics>,
    pub session_count: usize,
    pub averages: SessionAverages,
}

/// Saturating-add two `SessionMetrics` rows: every integer counter via `u64::saturating_add` (a
/// pathological/adversarial input can't wrap or panic), `cost_usd` via plain `f64` addition (no
/// saturation concept for a float ledger value).
fn add_saturating(a: &SessionMetrics, b: &SessionMetrics) -> SessionMetrics {
    SessionMetrics {
        input_tokens: a.input_tokens.saturating_add(b.input_tokens),
        output_tokens: a.output_tokens.saturating_add(b.output_tokens),
        total_tokens: a.total_tokens.saturating_add(b.total_tokens),
        cost_usd: a.cost_usd + b.cost_usd,
        wall_clock_ms: a.wall_clock_ms.saturating_add(b.wall_clock_ms),
        files_touched: a.files_touched.saturating_add(b.files_touched),
        churn_bytes: a.churn_bytes.saturating_add(b.churn_bytes),
        edit_count: a.edit_count.saturating_add(b.edit_count),
    }
}

/// Aggregate `(agent_id, model_id, SessionMetrics)` sessions into a [`FleetRollup`]: fleet totals,
/// per-model breakdown, per-agent breakdown, and division-safe averages. An empty slice yields an
/// all-zero `FleetRollup` (via `SessionAverages::default()`) rather than dividing by zero.
pub fn aggregate(sessions: &[(String, String, SessionMetrics)]) -> FleetRollup {
    let mut totals = SessionMetrics::default();
    let mut by_model: BTreeMap<String, SessionMetrics> = BTreeMap::new();
    let mut by_agent: BTreeMap<String, SessionMetrics> = BTreeMap::new();

    for (agent_id, model_id, metrics) in sessions {
        totals = add_saturating(&totals, metrics);

        let model_slot = by_model.entry(model_id.clone()).or_default();
        *model_slot = add_saturating(model_slot, metrics);

        let agent_slot = by_agent.entry(agent_id.clone()).or_default();
        *agent_slot = add_saturating(agent_slot, metrics);
    }

    let session_count = sessions.len();
    let averages = if session_count == 0 {
        SessionAverages::default()
    } else {
        let n = session_count as f64;
        SessionAverages {
            avg_cost_usd: totals.cost_usd / n,
            avg_tokens: totals.total_tokens as f64 / n,
            avg_wall_clock_ms: totals.wall_clock_ms as f64 / n,
            avg_files_touched: totals.files_touched as f64 / n,
            avg_churn_bytes: totals.churn_bytes as f64 / n,
        }
    };

    FleetRollup { totals, by_model, by_agent, session_count, averages }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
        wall_clock_ms: u64,
        files_touched: u64,
        churn_bytes: u64,
        edit_count: u64,
    ) -> SessionMetrics {
        SessionMetrics {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens.saturating_add(output_tokens),
            cost_usd,
            wall_clock_ms,
            files_touched,
            churn_bytes,
            edit_count,
        }
    }

    #[test]
    fn zero_sessions_yields_zero_totals_and_averages_no_div_by_zero() {
        let r = aggregate(&[]);
        assert_eq!(r.session_count, 0);
        assert_eq!(r.totals, SessionMetrics::default());
        assert!(r.by_model.is_empty());
        assert!(r.by_agent.is_empty());
        assert_eq!(r.averages, SessionAverages::default());
        assert_eq!(r.averages.avg_cost_usd, 0.0);
        assert_eq!(r.averages.avg_tokens, 0.0);
        assert_eq!(r.averages.avg_wall_clock_ms, 0.0);
        assert_eq!(r.averages.avg_files_touched, 0.0);
        assert_eq!(r.averages.avg_churn_bytes, 0.0);
    }

    #[test]
    fn totals_are_saturating_sum_of_all_sessions() {
        let sessions = vec![
            ("agent-a".to_string(), "anthropic/opus".to_string(), metrics(100, 50, 0.50, 1_000, 2, 500, 3)),
            ("agent-b".to_string(), "anthropic/opus".to_string(), metrics(200, 75, 0.75, 2_000, 1, 300, 4)),
            ("agent-a".to_string(), "openai/gpt".to_string(), metrics(50, 25, 0.10, 500, 3, 100, 1)),
        ];
        let r = aggregate(&sessions);

        assert_eq!(r.session_count, 3);
        assert_eq!(r.totals.input_tokens, 350);
        assert_eq!(r.totals.output_tokens, 150);
        assert_eq!(r.totals.total_tokens, 500);
        assert!((r.totals.cost_usd - 1.35).abs() < 1e-9, "got {}", r.totals.cost_usd);
        assert_eq!(r.totals.wall_clock_ms, 3_500);
        assert_eq!(r.totals.files_touched, 6);
        assert_eq!(r.totals.churn_bytes, 900);
        assert_eq!(r.totals.edit_count, 8);
    }

    #[test]
    fn per_model_and_per_agent_splits_are_correct_and_sorted() {
        let sessions = vec![
            ("agent-a".to_string(), "anthropic/opus".to_string(), metrics(100, 50, 0.50, 1_000, 2, 500, 3)),
            ("agent-b".to_string(), "anthropic/opus".to_string(), metrics(200, 75, 0.75, 2_000, 1, 300, 4)),
            ("agent-a".to_string(), "openai/gpt".to_string(), metrics(50, 25, 0.10, 500, 3, 100, 1)),
        ];
        let r = aggregate(&sessions);

        // Keys sorted (BTreeMap → deterministic).
        assert_eq!(r.by_model.keys().collect::<Vec<_>>(), vec!["anthropic/opus", "openai/gpt"]);
        assert_eq!(r.by_agent.keys().collect::<Vec<_>>(), vec!["agent-a", "agent-b"]);

        // anthropic/opus = session 1 + session 2.
        let opus = &r.by_model["anthropic/opus"];
        assert_eq!(opus.input_tokens, 300);
        assert_eq!(opus.output_tokens, 125);
        assert_eq!(opus.total_tokens, 425);
        assert!((opus.cost_usd - 1.25).abs() < 1e-9);
        assert_eq!(opus.wall_clock_ms, 3_000);

        // openai/gpt = session 3 alone.
        let gpt = &r.by_model["openai/gpt"];
        assert_eq!(gpt.input_tokens, 50);
        assert_eq!(gpt.total_tokens, 75);

        // agent-a = session 1 + session 3.
        let agent_a = &r.by_agent["agent-a"];
        assert_eq!(agent_a.input_tokens, 150);
        assert_eq!(agent_a.output_tokens, 75);
        assert_eq!(agent_a.files_touched, 5);
        assert_eq!(agent_a.edit_count, 4);

        // agent-b = session 2 alone.
        let agent_b = &r.by_agent["agent-b"];
        assert_eq!(agent_b.input_tokens, 200);
        assert_eq!(agent_b.wall_clock_ms, 2_000);
    }

    #[test]
    fn averages_are_correct_for_a_known_set() {
        let sessions = vec![
            ("agent-a".to_string(), "anthropic/opus".to_string(), metrics(100, 100, 1.00, 1_000, 2, 200, 2)),
            ("agent-b".to_string(), "anthropic/opus".to_string(), metrics(300, 100, 3.00, 3_000, 4, 600, 6)),
        ];
        let r = aggregate(&sessions);

        // totals: tokens 600, cost 4.00, wall_clock 4000, files 6, churn 800; count 2.
        assert_eq!(r.session_count, 2);
        assert!((r.averages.avg_cost_usd - 2.00).abs() < 1e-9, "got {}", r.averages.avg_cost_usd);
        assert!((r.averages.avg_tokens - 300.0).abs() < 1e-9, "got {}", r.averages.avg_tokens);
        assert!((r.averages.avg_wall_clock_ms - 2_000.0).abs() < 1e-9);
        assert!((r.averages.avg_files_touched - 3.0).abs() < 1e-9);
        assert!((r.averages.avg_churn_bytes - 400.0).abs() < 1e-9);
    }

    #[test]
    fn two_u64_max_sessions_saturate_without_panic() {
        let m = metrics(u64::MAX, u64::MAX, 1.0, u64::MAX, u64::MAX, u64::MAX, u64::MAX);
        let sessions =
            vec![("agent-a".to_string(), "model-x".to_string(), m.clone()), ("agent-a".to_string(), "model-x".to_string(), m)];
        let r = aggregate(&sessions);

        assert_eq!(r.totals.input_tokens, u64::MAX);
        assert_eq!(r.totals.output_tokens, u64::MAX);
        assert_eq!(r.totals.total_tokens, u64::MAX);
        assert_eq!(r.totals.wall_clock_ms, u64::MAX);
        assert_eq!(r.totals.files_touched, u64::MAX);
        assert_eq!(r.totals.churn_bytes, u64::MAX);
        assert_eq!(r.totals.edit_count, u64::MAX);
        assert!((r.totals.cost_usd - 2.0).abs() < 1e-9);
    }

    #[test]
    fn single_session_rollup_matches_the_session_itself() {
        let m = metrics(10, 20, 0.30, 40, 1, 50, 2);
        let sessions = vec![("solo-agent".to_string(), "solo-model".to_string(), m.clone())];
        let r = aggregate(&sessions);

        assert_eq!(r.session_count, 1);
        assert_eq!(r.totals, m.clone());
        assert_eq!(r.by_agent["solo-agent"], m.clone());
        assert_eq!(r.by_model["solo-model"], m);
    }
}
