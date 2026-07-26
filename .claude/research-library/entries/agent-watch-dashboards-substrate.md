---
title: "What substrate exists for Agent-Watch dashboards (replay scrubber, conflict radar, cost ledger) and how do we slice them?"
date: 2026-07-26
tags: [agent-watch, dashboards, replay-scrubber, conflict-radar, cost-ledger, sidecar, ai-console, events, cpe-396, cpe-731, gui]
status: current
---

## Question
For richer Agent-Watch dashboards — a replay scrubber, a conflict radar, a cost ledger — what already exists,
what needs new backend, and in what order should they ship?

## Finding (short)
- **Replay scrubber → pure frontend, buildable NOW.** `agentTimeline` (durable, timestamped, cap 300) is
  already a scrubbable event log; `agentDiffs` gives latest-only before/after per path. Ship first (zero
  backend), validates growing the AgentTimeline drawer into a tabbed dashboard. Caveat: files edited >1×/session
  only retain the newest diff — badge those, or add a per-path diff history later.
- **Cost ledger → backend bridge first, then panel.** The sidecar compute is ALL built + tested but wired to
  nothing: `sidecar/ai-console/src/session_metrics.rs` (`fold_session→SessionMetrics{tokens,cost_usd,wall_clock_ms,
  files_touched,churn_bytes,edit_count}`, CPE-1071), `cost.rs` (`rollup`, `budget_status`, CPE-913, doc says
  "advisory, never billing"), `efficiency.rs` (CPE-1074). Gap: nothing emits it — the host↔sidecar wire
  vocabulary is ONLY `session:` and `fs-read:` prefixes today. Need: (A) sidecar populates RunRecords from live
  provider responses (verify a real call site exists) + emits `cost:<json>` per session; host bridges it as a new
  `ai-console://agent-cost` event (same shape as the CPE-405 `fs-read:` bridge in `src-tauri/src/lib.rs`);
  (B) frontend `agentCost.ts` store (mirror `agentActivity.ts`) + ledger panel. Second — small lift, math done.
- **Conflict radar → biggest lift, do LAST, risks being misleading.** Today `AgentWatchState`
  (`src-tauri/src/lib.rs:4127`) watches ONE folder for ONE agent at a time (`agent_watch_start` replaces, not
  adds); `fs-activity` batches carry NO actor identity (`classify_fs_event:4142` sees raw OS events — a user's own
  edit is indistinguishable from an agent write). So "two agents touching the same path" is unobservable now.
  Needs: (A) generalize `AgentWatchState` to a keyed `HashMap<sessionId,AgentWatch>` (changes `agent_watch_start`
  signature + "replaces" semantics), tag every `fs-activity`/`fs-diff` batch with `sessionId`, decide actor
  attribution (app-initiated rename/move/delete = free "user" tag; unrelated process = "unknown actor", NOT
  silently the agent); (B) frontend same-path-within-window fold. `sidecar/ai-console/src/swarm_locks.rs`
  (`claims_overlap`) is architecturally close but sidecar-internal + glob-shaped, not reusable as-is.

## Substrate (confirmed, file:line)
- Backend (feature `sidecar-platform`, `src-tauri/src/lib.rs`): `AgentWatchState` (4127, single `Mutex<Option>`),
  `agent_watch_start(path)`/`agent_watch_stop` (4276/…, bound `bindings.gen.ts:1378,1389`), `fs_activity_pump`
  (4176, 200ms coalesce, cap 500), `classify_fs_event` (4142, created/modified/removed/renamed; reads dropped),
  `flush_fs_batch` (4228) emits `ai-console://fs-activity` (`Vec<{kind,path}>`) + `ai-console://fs-diff`
  (`Vec<{path,before,after}>`, CPE-743 via `agent_shadow::ShadowStore`). Session bridge (4078-4106): sidecar
  free-text `Status{state}` → host matches `session:<json>`→`ai-console://session`, `fs-read:<json>`→merged into
  fs-activity as `{kind:"read"}` (CPE-405). **Those two prefixes are the whole wire vocabulary.**
- Frontend: `src/lib/sidecar.ts` (AgentSession, FsActivity, start/stopAgentWatch), `agentSessions.ts`
  (`watchTargetFor` picks the single deepest-matching cwd — the "one watch" chokepoint), `agentActivity.ts`
  (`fsActivity` TTL 6s / `agentTimeline` cap 300 / `agentConsulted` cap 500, all in-memory, `clearActivity()` on
  stop), `agentDiffs.ts` (`agentDiffs` latest-only per path, cap 200/4MB, `foldDiffs` overwrites).
- Components/mount: `ExplorerPane.svelte:255` `.agent-strip` thin banner (`{#if activeWatchCwd}`; NOTE it violates
  the pills-reflow rule — `overflow:hidden;white-space:nowrap`); `AgentTimeline.svelte` fixed right drawer 340px
  (`App.svelte:3456`, `{#if activeWatchCwd && showTimeline}`) already holds timeline + ConsultedFiles + DiffPeek —
  the natural place to grow a **tabbed** dashboard.
- Mode gate: **no explicit toggle** — activates when `currentPath` falls inside a running session's cwd
  (`syncAgentWatch`, `App.svelte:790-809`). "Off means off": no sessions ⇒ `watchTargetFor`="" ⇒ watch never
  starts ⇒ zero threads/listeners. Every new store MUST attach its `listen()` only inside the `if(cwd)` branch
  and clear in `else` (like `initAgentActivity`/`initAgentDiffs`).

## Conventions
Event-driven (not STREAMING.md channels) — new data = new `ai-console://agent-cost`/`agent-conflict` events
bridged like the existing ones. Tracked `invoke` for watch start/stop. Theme vars only. Any chip row must
reflow (tick-tacks). AGENT-WATCH.md precedence: inside the mode visibility > fast/small/predictable, but
OFF must be absolutely zero-cost.

## Recommended order + ticket slicing
1. **Replay scrubber** — pure frontend, no backend ticket. First (lowest risk, validates the tabbed-drawer mount).
2. **Cost ledger** — sub-ticket A (sidecar RunRecord + `cost:` emit; host `ai-console://agent-cost` bridge),
   sub-ticket B (frontend `agentCost.ts` + panel). Second.
3. **Conflict radar** — sub-ticket A (multi-session `AgentWatchState` + per-event `sessionId`/actor tag),
   sub-ticket B (frontend overlap fold). Last — biggest change + easiest to ship misleadingly (hedge as
   "activity overlap" until real actor tagging lands).

## Risks
Off-mode leanness is the hard constraint on the radar (N watcher threads when N agents run, even if one on
screen — explicit product call: watch-all vs watch-overlapping). Radar false-attribution without actor tags.
Scrubber fidelity ceiling (latest-only diffs — badge multiply-edited files). Cost ledger is advisory, not billing.
