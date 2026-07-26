---
id: CPE-1097
title: "Agent Watch: bridge per-session cost metrics to the frontend (agent-cost event)"
type: feature
component: Backend
priority: medium
status: Backlog
tags: ready
created: 2026-07-26
epic: CPE-396
---

## Summary
GUI #3, cost-ledger slice A (enablement). The sidecar already computes everything a cost ledger needs —
`sidecar/ai-console/src/session_metrics.rs` (`fold_session → SessionMetrics {input_tokens, output_tokens,
total_tokens, cost_usd, wall_clock_ms, files_touched, churn_bytes, edit_count}`, CPE-1071), `cost.rs`
(`rollup`, `budget_status`, CPE-913 — "advisory, never billing"), `efficiency.rs` (CPE-1074) — but **none of
it reaches the host or frontend**: the host↔sidecar wire vocabulary is only the `session:` and `fs-read:`
prefixes today. Add the bridge so a per-session cost signal flows to the app. Backend/sidecar only; the panel
is CPE-1098. Full context: `.claude/research-library/entries/agent-watch-dashboards-substrate.md`.

## Design (buildable)
1. **Sidecar emit** — confirm `RunRecord`s are populated from real provider responses (verify a live call site
   exists in `sidecar/ai-console/src`; if only test fixtures construct them, that gap is part of this ticket —
   populate per-run token/cost/time from the actual API response). Then emit a per-session `Status` frame with
   a new prefix, e.g. `cost:<json SessionMetrics + rollup/budget>`, keyed by `sessionId`, on a sensible cadence
   (per run completion and/or throttled).
2. **Host bridge** — in `src-tauri/src/lib.rs`, in the session-announce matcher (~lines 4078-4106, alongside
   the existing `session:` → `ai-console://session` and `fs-read:` → fs-activity handlers), match the `cost:`
   prefix and re-emit as a new Tauri event `ai-console://agent-cost` carrying `{ sessionId, metrics }`. Mirror
   the CPE-405 `fs-read:` bridge exactly (small, precedented).
3. **Types** — a serde/specta struct crossing the boundary for the emitted payload (reuse/alias the sidecar
   `SessionMetrics`/`CostRollup`/`BudgetStatus` shapes; add plain derives where needed for the host side). No
   billing semantics — carry the "advisory" framing through.

## ⚠ Notes / guardrails
- **Off-means-off is absolute**: the emit path costs nothing when no session runs; the host listener follows
  the existing gate. No new deps. Event-driven (not STREAMING.md channels) — matches the existing Agent-Watch
  idiom. Async where any I/O is involved.
- Advisory data only — never present as billing.

## Acceptance Criteria
- [ ] The sidecar emits a `cost:` status per session from real run data (or the RunRecord-population gap is
      closed as part of this); the host re-emits it as `ai-console://agent-cost` keyed by `sessionId`.
- [ ] Payload struct is serde/specta-bound; no new deps; clippy clean (default + `--features index` where
      relevant) + sidecar clippy clean; existing sidecar cost/metrics tests still green.
- [ ] Zero cost when no session is running (no emit, no listener work).

## Work Log
2026-07-26 (workshift, GUI) — Filed as GUI #3 cost-ledger enablement from the Library substrate brief. Blocks
CPE-1098 (the ledger panel). Cut just-in-time; larger than the scrubber (CPE-1094), so scrubber ships first.
