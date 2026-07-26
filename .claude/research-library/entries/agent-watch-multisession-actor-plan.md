---
title: "How to make Agent Watch multi-session + actor-tagged (conflict-radar enablement) — build spec"
date: 2026-07-26
tags: [agent-watch, conflict-radar, multi-session, actor-tags, AgentWatchState, notify-watcher, off-means-off, cpe-1099, cpe-1100]
status: current
---

## Question
CPE-1099: how to turn single-session Agent Watch into multi-session watching + per-event actor tags so the
conflict radar (CPE-1100) shows real, non-misleading data — without breaking the off-means-off invariant?

## Decision (the key product call): WATCH ALL running sessions, unconditionally
Not "only cwd-overlapping sessions." Overlap detection is the radar's *frontend fold*, not a watch-selection
concern — you can't know a write overlaps until you observe it, and two agents in sibling cwds writing a
shared absolute path (monorepo `/shared`) are exactly the conflict to catch. Cost: **2N threads** for N
running sessions (1 `notify` watcher + 1 pump each; Windows `ReadDirectoryChangesW`). N is typically 1–4,
idle cost ≈ 0 (blocking wait), and it **still collapses to 0 when no session runs**. Nested watches emit
duplicate events for a shared subtree — desirable: each is tagged with its own session (that IS the signal),
bounded by the existing 200ms pump coalesce.

## Slice into 2 required + 1 optional
- **CPE-1099a — multi-session watch (structural).** `AgentWatchState.watches:
  Mutex<HashMap<sessionId, AgentWatch>>`; `agent_watch_start(session_id, path)` ADD-not-replace;
  `agent_watch_stop(session_id)` + new `agent_watch_stop_all`; frontend `watchTargets(sessions)=all` (keep
  `watchTargetFor` for the UI's `activeWatchCwd`); `syncAgentWatch`→ desired-set−armed-set **reconcile**
  (start/stop only deltas); **ONE** shared `fs-activity`+`fs-diff` listener gated on `armed.size>0` (NOT per
  session); regen bindings (`agentWatchStart`/`agentWatchStop` arg lists change). Event shape unchanged →
  1-session behaviour byte-identical; low blast radius. **Highest-risk slice** (edits the #413 lib.rs region;
  must preserve off-means-off across session churn + N-watch reconcile with no leaked listeners/threads).
- **CPE-1099b — per-event actor tag.** `actor` field on fs-activity/fs-diff items (`flush_fs_batch:4272`):
  sessionId (pump owns its id) / `"user"` / `"unknown"`. `"user"` via an **app-op ledger**
  (`Mutex<VecDeque<(path, Instant)>>` on state, **no thread/timer** — aged in-line during flush) recorded by a
  `note_app_op(app,&[paths])` called at the 8 file-op `*_impl` sites (rename/delete/copy/move/move_exact/
  create_dir/create_file/write_file_text). **Wiring subtlety:** those cmds are default-build but
  `AgentWatchState` is `#[cfg(sidecar-platform)]` → make `note_app_op` a no-op without the feature (plain
  explorer pays nothing). Add `sessionId` to `read_announcement` (console.rs:177) so reads attribute. Thread
  `actor` through `sidecar.ts FsActivity`/`agentActivity.ts`/`agentDiffs.ts` (default `"unknown"` if absent).
- **CPE-1099c (optional/deferrable) — honest unknown-vs-agent.** Sidecar `fs-write:<json{sessionId,path}>`
  reporting; then a watcher write is `actor=sessionId` only if it matches a recent agent-reported write, else
  `"unknown"` — positively satisfies "unrelated process ≠ agent." If deferred, hedge CPE-1100's UI copy
  ("activity under <agent>'s folder") and document the fidelity ceiling.

## Off-mode leanness inventory (the hard constraint)
Backend per watched session: 1 notify watcher (~1 OS thread) + 1 `fs_activity_pump` thread. Collapse:
remove key → drop `AgentWatch` → `_watcher` tx closes → pump `recv_timeout` Disconnected → final flush +
break → thread exits. Empty map = 0 threads; the two Mutexes are zero-cost empty; **app-op ledger adds no
thread/timer**. Frontend: exactly 2 listeners + 1 prune interval when armed set non-empty, torn down to 0
when empty — independent of N (never per-session). Add invariant tests: arm 2 → stop_all →
`watches.is_empty()`; no session → `watchTargets` empty → `startAgentWatch` never called.

## Rebase note
Build AFTER PR #413 merges (adds `cost:` arm at lib.rs:4125-4150 + fixes the announcer double-prefix so
`fs-read:` is live). Expect a trivial textual conflict only in the matcher block.

## What CPE-1100 (radar) then consumes
The actor-tagged `ai-console://fs-activity` stream (agentTimeline/fsActivity now carry `actor`), folded
same-path-within-window across **distinct sessionIds → conflict**, joined to `agentSessions` for names,
rendered as a new tab in the `AgentTimeline.svelte` drawer. No further backend once 1099a+b land.

## Critical files
`src-tauri/src/lib.rs` (AgentWatchState :4169, agent_watch_start :4318, agent_watch_stop :4344,
fs_activity_pump :4220, flush_fs_batch :4272, matcher :4125, file-op *_impl 1136/1168/1384/1413/1821);
`src/App.svelte` (syncAgentWatch/reconcile :795-816); `src/lib/agentSessions.ts` (watchTargetFor/watchTargets
:36); `src/lib/sidecar.ts` (FsActivity/start-stop/normalizeFsActivity :152-191); `sidecar/ai-console/src/console.rs`
(read_announcement :177; optional fs-write: for 1099c).
