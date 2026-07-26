---
id: CPE-1098
title: "Agent Watch: cost ledger panel (live tokens + USD per session)"
type: feature
component: Frontend
priority: medium
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-396
depends-on: CPE-1097
---

## Summary
GUI #3, cost-ledger slice B (the panel). Render the per-session usage bridged by CPE-1097
(`ai-console://agent-cost` → `{ sessionId, inputTokens, outputTokens, costUsd }`, live from the sidecar's
`usage.rs` PTY scrape) as a **cost ledger** tab in the Agent Watch dashboard. Frontend only. **Scope note**
(see `.claude/research-library/entries/sidecar-live-cost-usage-scanner.md`): the live signal is only 3 fields
(input/output tokens + cost_usd), best-effort and **advisory** — the richer `SessionMetrics` ledger
(wall-clock, files-touched, churn, per-model rollup, budget status) is unwired dead code and explicitly OUT of
scope here; a future ticket can add it if live capture is ever built.

## Design (buildable)
1. **`src/lib/agentCost.ts`** — a store mirroring `src/lib/agentActivity.ts`'s init/clear/fold pattern:
   attach a `listen("ai-console://agent-cost", …)` ONLY inside the same `if (cwd)` gate as
   `initAgentActivity`/`initAgentDiffs` (`App.svelte:790-809`), fold incoming `{sessionId, inputTokens,
   outputTokens, costUsd}` per `sessionId` (latest wins), and `clear()` on stop-watching. Zero cost when not
   watching (off-means-off is absolute); remove the listener on stop/destroy (no leak).
2. **Ledger tab** in the Agent Watch dashboard drawer (`AgentTimeline.svelte`, grown into a tabbed dashboard
   by CPE-1094 — reuse that tab host; TABS.md `.tab`/`.tab.active`). Show the current session's usage as
   labelled rows/cards: **input tokens**, **output tokens**, **total tokens**, **cost (USD)** — formatted
   (thousands separators; cost to 2-4 dp). If multiple sessions report, list them or offer a switcher.
3. **Advisory framing** — a small note that these are best-effort figures scraped from the agent's output, not
   billing. Theme vars only (MENUS.md); any chip row reflows (tick-tacks); division/NaN-safe on any derived
   value (e.g. a tokens/sec or in:out ratio if shown → 0-guarded, never NaN).

## ⚠ Notes / guardrails
- Pure store + a component. No new deps. Theme vars only; advisory framing (never "billing"). Off-means-off:
  nothing allocated / no listener when `activeWatchCwd === ""`.
- Do NOT render `SessionMetrics`/rollup/budget fields — the bridge (CPE-1097) doesn't carry them.
- If this adds a user-facing section, honour the self-maintaining-docs rule (likely a note on the Agent Watch
  docs page, not a new Section slug — use judgment).

## Acceptance Criteria
- [ ] While watching an agent that reports usage, the dashboard shows a cost ledger tab with live input/output/
      total tokens + USD cost, updating as `agent-cost` events arrive; advisory note present.
- [ ] No data / not watching → no panel, no error, zero listener cost; any derived ratio is NaN-safe.
- [ ] Colours from theme vars (identical light/dark); pills reflow; `npm run check` clean; vitest green (store
      fold/clear + formatting/NaN-safe helpers tested); no new deps.

## Work Log
2026-07-26 (workshift, GUI) — Filed as GUI #3 cost-ledger panel on top of CPE-1097's bridge; **rescoped** to the
live `usage.rs` 3-field signal (tokens + cost) per the de-risk spike, dropping the unwired SessionMetrics
ledger. Cut just-in-time after the scrubber (CPE-1094).
