---
id: CPE-731
title: "EPIC: Agent cost & resource dashboard"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
A per-session and cross-session dashboard of what the work is *costing*: tokens, model spend, wall-clock,
files touched, edit churn, and throughput over time — turning the Agents left-pane list into an accountable
ledger.

## Why
A developer running agents wants to know which agent/model is burning budget for how much filesystem
progress. Making cost and throughput visible closes the loop between "what the agent did" and "what it cost".

## Rough scope (areas, not child tickets)
- A usage/metrics event source from the sidecar (tokens, model, timing).
- A rollup store aggregating per-session and cross-session metrics.
- A dashboard with sparklines/tiles (follows the dataviz conventions).
- Cost-per-progress framing (spend vs. files touched / churn).

## Open questions (resolve at activation)
- What usage data the console/sidecar can emit (tokens, cost) and how reliably.
- Pricing source for spend estimates and keeping it current.
- Retention/aggregation window for historical metrics.

## Definition of Done
- Per-session and cross-session token/spend/time/files/churn/throughput are shown in a dashboard.
- Metrics stream from the sidecar and roll up historically.
- With no agent running, the dashboard is idle and adds no explorer cost.

## Work Log
2026-07-22 (nightshift) — **Activated.** Grep-first: the sidecar already tracks per-run tokens
(`Usage`) + per-token model prices (`model_catalog::Pricing`), but never combined them — reported
`cost_usd` comes only from the agent's own observability, which not every agent emits. First slice
shipped: **CPE-912** — `Pricing::estimate_cost(input_tokens, output_tokens)` (tokens × price). Remaining
is the dashboard GUI (per-session / rolling cost, budgets + alerts).
