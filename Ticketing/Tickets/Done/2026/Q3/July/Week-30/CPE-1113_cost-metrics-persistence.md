---
id: CPE-1113
title: "Cost dashboard: per-session metrics journal + flush-on-session-end"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-731
---

## Summary
CPE-731 slice **b** (persistence — the highest-risk slice). Persist a per-session `SessionMetricsRecord` when
a watched session ends, so the cost dashboard can show **cross-session history** (731c). Reuse CPE-733's
audit-journal PATTERN as a **sibling** per-session journal (one row/session), NOT the event journal. Builds on
CPE-1107 (the live `agentSessionMetrics` accumulator). Design:
`.claude/research-library/entries/cost-dashboard-full-ledger-history-plan.md` (§3-4).

## Context (verified — file:line)
- `crates/server/src/audit_journal.rs` — the PATTERN to mirror (JSONL, one file/session, temp-file+rename,
  sanitized filename, cap/trim). Commands `audit_record`/`audit_read` (src-tauri/src/lib.rs ~:2377-2417),
  `audit_dir(app)` (~:2370).
- `src/lib/agentSessionMetrics.ts` (CPE-1107) — the live accumulator + `SessionMetrics` shape (identity +
  metrics). Flush seam: `reconcileAgentWatch → stopAgentWatch(id)` (App.svelte ~:825-829) + full-stop
  `clearAgentSessionMetrics` — the per-session stop does NOT clear `agentCost` (only full teardown does), so
  the data is present at that seam.

## Design (buildable)
1. **`crates/server/src/metrics_journal.rs`** (new, mirror `audit_journal.rs`): `#[derive(Serialize,
   Deserialize, specta::Type)] pub struct SessionMetricsRecord { sessionId, agentId, agentName, provider,
   model, cwd, startedAt, endedAt, wallClockMs, inputTokens, outputTokens, totalTokens, costUsd, filesTouched,
   churnBytes, editCount }`. `append(base, &SessionMetricsRecord, max_rows) -> Result<(),String>` to a SINGLE
   append-only `history.jsonl` under an `agent-metrics/` dir (one line/session; bounded/rotated by `max_rows`,
   copy audit_journal's trim). `read_all(base) -> Vec<SessionMetricsRecord>`. Pure; unit-tested (append N,
   round-trip, cap-rotation, corrupt-line-skipped).
2. **Commands** in `src-tauri/src/lib.rs` (thin spawn_blocking, mirror audit_record/read): `metrics_record(rec:
   SessionMetricsRecord) -> Result<(),String>` (append to `<app_data>/agent-metrics/history.jsonl`),
   `metrics_history() -> Result<Vec<SessionMetricsRecord>,String>`. Use a `metrics_dir(app)` sibling of
   `audit_dir`. Register in `generate_handler!` + specta list; regen bindings (`metricsRecord`/`metricsHistory`
   + `SessionMetricsRecord`); drift-guard passes.
3. **Flush-on-end (frontend)** — in `agentSessionMetrics.ts`/`App.svelte`: when a session leaves the watched set
   (`stopAgentWatch(id)` in `reconcileAgentWatch`) and at full stop (`clearAgentSessions`), build the
   `SessionMetricsRecord` from the accumulator (`deriveSessionMetrics` + identity + endedAt/wallClock) and call
   `commands.metricsRecord(rec)` **before** the stores clear. Guard: only flush a session that actually
   produced metrics (avoid empty rows); flush once per session end (don't double-write). Errors logged/swallowed
   (a persistence failure must not break teardown).

## ⚠ Guardrails
- **Flush timing is the top risk**: capture the record from the live accumulator (which holds its own copy) and
  flush at the `stopAgentWatch(id)` seam + full-stop loop BEFORE stores clear. Flush once per end; skip
  empty/never-started sessions. Errors swallowed.
- Off-means-off: the journal is written only on a session END (no session ⇒ no write); `metrics_history` is
  pull-only (read on dashboard open, 731c). No new deps. Advisory framing (best-effort) carried into the record.
- SIBLING journal (`agent-metrics/history.jsonl`), NOT the audit event journal — grain mismatch (per-session vs
  per-event). Do NOT fold into the audit file.

## Acceptance Criteria
- [ ] `metrics_journal` appends a per-session `SessionMetricsRecord` to a sibling `history.jsonl` (bounded/
      rotated), `read_all` returns them; `metrics_record`/`metrics_history` commands registered + bindings
      regenerated + drift-guard passes. A session end flushes exactly one record (built from the accumulator
      before clear); empty/never-started sessions are not written; nothing is written when no session ends.
- [ ] Flush + journal errors are logged/swallowed (teardown/live view unaffected). `cargo test -p cpe-server`
      green (append/round-trip/cap/corrupt-skip); clippy clean (default + index + sidecar-platform);
      `npm run check` clean; `npm test` green (flush-builds-right-record + flush-once tests); no new deps.

## Work Log
2026-07-26 (sprint) — CPE-731 slice b, from the filed plan. Sibling metrics_journal (audit-journal pattern) +
flush-on-end from the CPE-1107 accumulator. Unblocks 731c (cross-session rollup dashboard). Coordinates with
CPE-728's audit journal — separate file, same app-data root.
