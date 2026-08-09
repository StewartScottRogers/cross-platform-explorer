---
id: CPE-1097
title: "Agent Watch: bridge live per-session usage (tokens + cost) to the frontend (agent-cost event)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-396
---

## Summary
GUI #3, cost-ledger slice A (enablement). Bridge the sidecar's **already-live** per-session usage —
`usage.rs`'s `Usage { input_tokens, output_tokens, cost_usd }`, scraped live from the agent's PTY output —
through to the host/frontend as a new `ai-console://agent-cost` event, so a cost-ledger panel (CPE-1098) can
render it. Sidecar + host only. **Scope confirmed by research** (see
`.claude/research-library/entries/sidecar-live-cost-usage-scanner.md`): bridge the live `usage.rs` data —
do NOT scope in `session_metrics.rs`/`RunRecord`/`cost.rs`/`fleet_metrics.rs`, which are well-tested but
**unwired** (no live `RunRecord` is ever constructed; folding them in needs a separate, larger capture effort).

## Context (verified — file:line)
- Live capture: `sidecar/ai-console/src/usage.rs` — `Usage { input_tokens:u64, output_tokens:u64,
  cost_usd:f64 }` (:20-28); `UsageScanner::new()` (`console.rs:249`) + `*usage.lock().unwrap() =
  usage_scan.feed(&text)` (`console.rs:280`) in the per-session reader loop over real agent stdout. Best-effort
  regex scrape (`usage.rs:144-159`), since agents run in a PTY (no structured API response).
- Emit seam: `SessionAnnouncer = Arc<dyn Fn(String)+Send+Sync>` (`console.rs:98`); `session:` built at
  `main.rs:288-293`, `fs-read:` at `console.rs:177-180` + fired at `console.rs:277`. `usage_json`
  (`console.rs:184-190`) already serializes `inputTokens`/`outputTokens`/`costUsd`.
- Host bridge: `src-tauri/src/lib.rs` session-announce matcher (~4078-4106) turns `session:`→
  `ai-console://session` and `fs-read:`→fs-activity (CPE-405). Add a `cost:` arm the same way.

## Design (buildable)
1. **Sidecar emit** — right after `console.rs:280`, `announce(format!("cost:{}", usage_json(&usage.lock()...)))`
   (the `announce` closure + session id `record_id`/`diag_id` at `console.rs:242-244` are already in scope).
   Add a `sessionId` field to the `usage_json` payload (mirror `session_payload` at `console.rs:200-211`) so
   the host can key by session. Emit on meaningful change (don't spam every chunk — e.g. only when the scanned
   `Usage` actually changed, or throttled).
2. **Host bridge** — in `src-tauri/src/lib.rs`, match the `cost:` prefix beside the existing `session:`/
   `fs-read:` handlers and re-emit as `ai-console://agent-cost` carrying `{ sessionId, inputTokens,
   outputTokens, costUsd }`. Mirror the CPE-405 `fs-read:` bridge exactly.
3. **Types** — a small serde/specta struct for the emitted payload on the host side (plain, advisory). No
   billing semantics.

## ⚠ Notes / guardrails
- **Off-means-off**: the emit only fires inside an active session's reader thread (already gated); nothing new
  runs when no session is watched. No new deps. Event-driven (not STREAMING.md channels).
- Advisory data only (best-effort PTY scrape) — never present as billing; the panel must frame it that way.
- Do NOT wire `RunRecord`/`session_metrics`/`fleet_metrics` — out of scope (unreachable today; separate ticket).

## Acceptance Criteria
- [ ] The sidecar emits `cost:<json {sessionId, inputTokens, outputTokens, costUsd}>` when a session's scanned
      usage changes; the host re-emits it as `ai-console://agent-cost` keyed by `sessionId`.
- [ ] Payload struct is serde/specta-bound (host side); no new deps; sidecar + app clippy clean (default +
      `--features index`/`sidecar-platform` as relevant); existing `usage.rs` tests still green.
- [ ] Zero new cost when no session runs (emit lives in the existing per-session reader thread only).

## Work Log
2026-07-26 (sprint, GUI) — Filed as GUI #3 cost-ledger enablement; **rescoped after a de-risk spike** (Library
entry `sidecar-live-cost-usage-scanner.md`) from the unwired `session_metrics`/`RunRecord` stack to the
actually-live `usage.rs` `UsageScanner`. Blocks CPE-1098 (the panel). Cut just-in-time after the scrubber.

2026-07-26 — Merged (PR #413). Reviewer APPROVE + UAT PASS (after a Foreman fix typing the host payload as
`AgentCostEvent`). **Bonus fix bundled:** the worker discovered `main.rs` blanket-wrapped every announcement
in `session:`, so `fs-read:` came out as `session:fs-read:{…}` and never matched the host arm — meaning the
Agent-Watch "read" activity kind (CPE-405) had been **silently broken in production**. Each announce builder
now self-prefixes; regression test `distinct_announcement_kinds_never_collide_on_prefix` added. This was a
genuine prerequisite (cost: would have inherited the same double-prefix).
