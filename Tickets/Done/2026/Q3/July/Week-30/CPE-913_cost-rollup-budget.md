---
id: CPE-913
title: Agent cost rollup + budget status (dashboard compute core)
type: feature
component: Sidecar
priority: low
tags: ready
epic: CPE-731
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
Second headless slice of the cost dashboard (CPE-731), building on the estimator (CPE-912). New
`ai-console::cost`:
- `rollup(runs)` — aggregate `(model_id, cost_usd)` into a `CostRollup { total_usd, by_model }`
  (per-model, `BTreeMap`-sorted for a stable dashboard order).
- `budget_status(spent, budget) -> BudgetStatus { remaining, fraction, level }` — the budget gauge, with
  `BudgetLevel::{Ok, Warn (≥80%), Over (≥100%)}`. A zero/negative budget = "no budget set" → `Ok`,
  fraction 0 (no div-by-zero); over-budget remaining goes negative.

## Acceptance Criteria
- [x] `rollup` totals + per-model breakdown (sorted, empty→zero).
- [x] `budget_status` levels (Ok/Warn/Over), remaining (incl. negative), and the no-budget/zero case.
- [x] 3 unit tests; clippy `-D warnings` clean.

## Work Log
- 2026-07-22 — The dashboard's pure compute core (totals, per-model split, budget alerts). The dashboard
  UI + wiring live runs into it is the GUI remainder of CPE-731.
