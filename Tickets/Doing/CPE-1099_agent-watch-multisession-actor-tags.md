---
id: CPE-1099
title: "Agent Watch: multi-session watching (conflict-radar enablement, slice a)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-396
---

## Summary
GUI #3 conflict-radar **slice (a)** — the structural refactor. Generalise Agent Watch from watching ONE
session's folder to watching **every running session** concurrently, keyed by `sessionId`. **Event payload
shape is UNCHANGED in this slice** (actor tags are slice b = CPE-1101), so with one session running the
behaviour is byte-identical to today — low blast radius. Full design + rationale:
`.claude/research-library/entries/agent-watch-multisession-actor-plan.md` (READ IT). Product decision (made):
**watch all running sessions**, not cwd-overlap-filtered (overlap detection is the radar's frontend fold).

## Context (verified — file:line, may shift slightly post-#413/#414)
- `src-tauri/src/lib.rs`: `AgentWatchState` (~:4169, `Mutex<Option<AgentWatch>>`), `agent_watch_start(path)`
  (~:4318, REPLACES the one watch), `agent_watch_stop()` (~:4344), `fs_activity_pump` (~:4220),
  `flush_fs_batch` (~:4272). Registered `AgentWatchState::default()` (~:5825); `generate_handler!` +
  specta lists (~:6026 / ~:6416).
- Frontend: `src/lib/agentSessions.ts` `watchTargetFor` (:36, returns the single deepest cwd);
  `src/App.svelte` `syncAgentWatch` (~:798-817, one-watch); `src/lib/sidecar.ts` `startAgentWatch`/
  `stopAgentWatch` (:159-175). Bindings `agentWatchStart`/`agentWatchStop` in `bindings.gen.ts`.

## Design (buildable — from the filed plan)
1. **State → keyed map.** `AgentWatchState.watches: Mutex<HashMap<String, AgentWatch>>` (key = sessionId).
   `AgentWatch { _watcher, path }` unchanged.
2. **`agent_watch_start(session_id: String, path: String)` — ADD not replace.** Build watcher + spawn
   `fs_activity_pump` as today; `watches.lock().insert(session_id, AgentWatch{..})`. Re-inserting a key drops
   the old watch (idempotent re-arm); a new key leaves existing watches running.
3. **`agent_watch_stop(session_id: String)`** removes just that key (drop → watcher+pump thread die). Add
   **`agent_watch_stop_all()`** = `watches.clear()`.
4. **Registration + bindings.** Add `agent_watch_stop_all` to `generate_handler!` + specta list; regenerate
   `bindings.gen.ts` (arg lists of `agentWatchStart`/`agentWatchStop` changed) via
   `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` (from src-tauri/); drift-guard must pass.
5. **Frontend reconcile.** `agentSessions.ts`: add pure `watchTargets(sessions) = sessions` (watch-all);
   KEEP `watchTargetFor` for the UI's `activeWatchCwd` (which project the on-screen drawer describes).
   `App.svelte` `syncAgentWatch` → a **reconcile** over desired session set vs. an armed `Map<sessionId,path>`:
   start the delta (`startAgentWatch(id, cwd)`), stop the removed (`stopAgentWatch(id)`). **Keep exactly ONE
   shared `fs-activity`+`fs-diff` listener pair (and the CPE-1094/1098 listeners)** gated on `armed.size>0` —
   NOT one per session. `sidecar.ts`: `startAgentWatch(sessionId, path)`, `stopAgentWatch(sessionId)`,
   new `stopAllAgentWatch()`.

## ⚠ Notes / guardrails (off-means-off is the hard constraint)
- Per watched session: 1 notify watcher + 1 pump thread. **Empty map ⇒ 0 threads.** Removing a key drops
  `AgentWatch` → tx closes → pump `recv_timeout` Disconnected → final flush + break → thread exits. The two
  Mutexes are zero-cost empty. NOTHING new runs when no session is active.
- Frontend: exactly 2 activity/diff listeners (+ the existing cost/session listeners) when armed set non-empty;
  torn down to 0 when empty — independent of N (never per-session).
- No new deps. async + spawn_blocking watcher setup preserved. **Event payload shape unchanged** (actor is
  slice b) — a regression here is isolated to watch lifecycle, not payload parsing.
- Keep `watchTargets`/`watchTargetFor` pure + unit-tested (`agentSessions.test.ts`).

## Acceptance Criteria
- [ ] Multiple running sessions are watched concurrently (keyed `HashMap`); `agent_watch_start(session_id,path)`
      ADDs, `agent_watch_stop(session_id)` removes just that one, `agent_watch_stop_all` clears; a Rust test
      arms two sessions then `stop_all` and asserts `watches.is_empty()` (zero-watcher invariant).
- [ ] Frontend reconcile starts/stops only the delta; exactly ONE shared activity/diff listener pair regardless
      of N; no session ⇒ `watchTargets` empty ⇒ `startAgentWatch` never called (zero threads/listeners).
- [ ] With a single session, behaviour is unchanged (event shape identical); existing agent-watch tests green.
- [ ] Bindings regenerated (`agentWatchStart(sessionId,path)`, `agentWatchStop(sessionId)`, `agentWatchStopAll`);
      drift-guard passes; `npm run check` clean; clippy clean (default + `--features index` + sidecar-platform);
      no new deps.

## Work Log
2026-07-26 (workshift, GUI) — Rescoped to slice (a) of the conflict-radar enablement per the filed build spec
(watch-all decision; keyed HashMap; off-mode collapses to 0 threads). Slice (b) actor tags = CPE-1101; the
radar panel = CPE-1100 (consumes a+b). Highest-risk slice — event shape kept unchanged to isolate risk to the
watch lifecycle.
