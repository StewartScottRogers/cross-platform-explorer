---
id: CPE-912
title: Agent cost estimator (tokens × catalog price)
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
First headless slice of the agent cost & resource dashboard (CPE-731). The sidecar already tracks a run's
tokens (`Usage { input_tokens, output_tokens }`) and holds per-token model prices (`model_catalog::Pricing
{ prompt, completion }`), but nothing combined them — the reported `cost_usd` comes only from the agent's
own observability output, which not every agent emits.

Added `Pricing::estimate_cost(input_tokens, output_tokens) -> Option<f64>`: `input × prompt + output ×
completion`. `None` when both prices are missing (nothing to estimate); a missing side counts as 0 so a
partial price still estimates; zero tokens with a price → `$0`, not `None`. Advisory display data — never
trusted for billing (matching the `Pricing` doc).

## Acceptance Criteria
- [x] `estimate_cost` = tokens × per-token price; partial/zero/missing-price cases handled sensibly.
- [x] Unit-tested (a $3/$15-per-1M example, partial price, no price, zero tokens); clippy `-D warnings` clean.

## Work Log
- 2026-07-22 — Activated CPE-731. The dashboard UI (per-session / rolling cost, budgets/alerts) is the GUI
  remainder; this fills the missing compute so cost shows even when the agent doesn't self-report it.
