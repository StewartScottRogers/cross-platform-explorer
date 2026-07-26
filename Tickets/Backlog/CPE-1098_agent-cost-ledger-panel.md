---
id: CPE-1098
title: "Agent Watch: cost ledger panel (tokens / USD / time per session)"
type: feature
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-07-26
epic: CPE-396
depends-on: CPE-1097
---

## Summary
GUI #3, cost-ledger slice B (the panel). Render the per-session cost metrics bridged by CPE-1097 as a **cost
ledger** in the Agent Watch dashboard: tokens (in/out/total), USD cost, wall-clock, files touched, churn,
edit count, plus a per-model rollup and a budget status pill. Frontend only. Context:
`.claude/research-library/entries/agent-watch-dashboards-substrate.md`.

## Design (buildable)
1. **`src/lib/agentCost.ts`** — a store mirroring `src/lib/agentActivity.ts`'s init/clear/fold pattern:
   attach a `listen("ai-console://agent-cost", …)` ONLY inside the same `if (cwd)` gate as
   `initAgentActivity`/`initAgentDiffs` (App.svelte:790-809), fold incoming `SessionMetrics` per `sessionId`,
   and `clear()` on stop-watching. Zero cost when not watching (off-means-off is absolute).
2. **Ledger panel** — a tab/section in the Agent Watch dashboard drawer (`AgentTimeline.svelte`, grown into a
   tabbed dashboard by CPE-1094 — reuse that tab host; TABS.md active-tab treatment). Show the current
   session's metrics as labelled rows/cards; a per-model rollup table (from `CostRollup`); a **budget status
   pill** coloured by `BudgetStatus` level (Ok/Warn/Over) using theme vars only (no hard-coded colours —
   MENUS.md). Any chip row reflows (tick-tacks). Frame it as **advisory**, not billing (carry the sidecar's
   own caveat into a small note).
3. **Multi-session** — if several sessions have cost data, list them (or show the active one + a switcher);
   division-safe on all ratios (0 files → no NaN in usd/file, tokens/file).

## ⚠ Notes / guardrails
- Pure store logic + a component. No new deps. Theme vars only; pills reflow. Division/NaN-safe. Advisory
  framing (never "billing"). Remove the listener on stop/destroy (no leak); nothing allocated when off.
- If this adds a user-facing section, honour the self-maintaining-docs rule (likely a note on the Agent Watch
  docs page rather than a new Section — use judgment).

## Acceptance Criteria
- [ ] While watching an agent that reports cost, the dashboard shows a cost ledger: tokens/USD/time/files +
      per-model rollup + a budget-status pill; updates live as new `agent-cost` events arrive.
- [ ] No data / not watching → no panel, no error, zero listener cost; ratios are NaN-safe.
- [ ] Colours from theme vars (identical light/dark); pills reflow; advisory framing present; `npm run check`
      clean; vitest green (store fold/clear + NaN-safe ratio helpers tested); no new deps.

## Work Log
2026-07-26 (workshift, GUI) — Filed as GUI #3 cost-ledger panel on top of CPE-1097's bridge. From the Library
substrate brief. Cut just-in-time after the scrubber (CPE-1094).
