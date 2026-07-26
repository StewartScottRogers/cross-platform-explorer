---
title: "Does the ai-console sidecar capture live cost/token data, and where does a cost: status emit hook in?"
date: 2026-07-26
tags: [agent-watch, cost-ledger, sidecar, ai-console, usage-scanner, session-metrics, runrecord, status-emit, cpe-1097]
status: current
---

## Question
For the Agent-Watch cost ledger (CPE-1097/1098): does the sidecar capture per-run cost/token data from live
agent runs, and where would a `cost:<json>` status emit sit beside the existing `session:`/`fs-read:` bridges?

## Finding (short) — corrects an earlier scoping assumption
- **`RunRecord` / `session_metrics.rs::fold_session` / `cost.rs::rollup` / `efficiency.rs` / `fleet_metrics.rs`
  are UNWIRED dead code.** Every construction/call site is inside `#[cfg(test)]` in those modules — no
  bin/`console.rs`/`session_engine.rs`/`main.rs` ever builds a `RunRecord` or folds a session. Well-tested,
  pure, but currently unreachable. **Do NOT scope CPE-1097 around these.**
- **What IS live: `usage.rs` `UsageScanner`.** `Usage { input_tokens:u64, output_tokens:u64, cost_usd:f64 }`
  (`usage.rs:20-28`), fed live per session in the reader thread: `usage_scan = UsageScanner::new()`
  (`console.rs:249`), `*usage.lock().unwrap() = usage_scan.feed(&text)` (`console.rs:280`) inside the
  `while let Ok(chunk) = rx.recv()` loop over real agent stdout. Capture is **best-effort regex text-scraping**
  of the agent CLI's printed usage lines (`usage.rs:144-159 parse_line`) — the sidecar spawns agents in a PTY
  (`session_engine.rs:78`) and never sees a structured API response (no HTTP LLM client in the crate). Today
  `Usage` is exposed only via the sidecar's OWN embedded HTTP UI (`GET /api/sessions` →
  `handle_sessions_list`, `console.rs:875-907`, serialized by `usage_json` `console.rs:184-190`) — it never
  reaches the host/Agent-Watch bridge.

## The status-emit seam (exact hook)
Host-bridged prefixes are built in the reader thread via `SessionAnnouncer = Arc<dyn Fn(String)+Send+Sync>`
(`console.rs:98`), threaded per session. Pattern to copy — `main.rs:288-293` emits `session:{payload}`;
`console.rs:177-180 read_announcement` builds `fs-read:` and fires it at `console.rs:277`. **Smallest hook:**
right after `console.rs:280`, `announce(format!("cost:{}", usage_json(&usage.lock().unwrap())))` — the
`announce` closure + the session id (`record_id`/`diag_id`, `console.rs:242-244`) are already in scope.
`usage_json` already emits `inputTokens`/`outputTokens`/`costUsd`; it just needs a `sessionId` field added
(mirror `session_payload` `console.rs:200-211`).

## Gap verdict
- **(a) achievable now, small/low-risk:** bridge the 3 live `usage.rs` fields (input tokens, output tokens,
  cost_usd) via a new `cost:<json>` Status emit → host re-emits `ai-console://agent-cost`. **Scope CPE-1097 as
  this.**
- **(b) bigger, out of scope:** the full `SessionMetrics` ledger (wall_clock, files_touched, churn_bytes,
  edit_count, per-model rollup, budget_status) needs live `RunRecord` capture that does not exist — agents are
  opaque PTY subprocesses, so there's no structured usage object; would require rigorous end-of-run parsing or
  calling providers directly (materially larger). Leave `session_metrics.rs` et al. unwired for now.

## Impact
CPE-1097 rewritten to bridge `usage.rs` `Usage` (not `session_metrics`). CPE-1098 panel renders input/output
tokens + cost_usd (advisory), not the richer ledger. Richer metrics = a separate future ticket.
