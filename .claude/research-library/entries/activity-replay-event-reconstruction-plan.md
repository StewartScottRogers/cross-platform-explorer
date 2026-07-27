---
title: "How to finish CPE-728 activity-replay folder reconstruction (event-replay approach)?"
date: 2026-07-26
tags: [activity-replay, event-sourcing, audit-journal, replay, reconstruction, cpe-728, cpe-733, agent-watch]
status: current
---

## Decision (user, 2026-07-26): event-replay — reconstruct folder state at T by replaying events off a baseline.

## KEY FINDING: the backend is ~80% built + unit-tested, just UNWIRED
Two tracks were built separately and never joined:
- **Track A (built, unwired):** `crates/server/src/audit_journal.rs` (append-only JSONL, one file/session,
  bounded MAX_EVENTS_PER_SESSION=10_000; `AuditEvent{ts,session,kind,path,detail}` :21-33; record/read_session/
  list_sessions). **The reconstruction fold already exists**: `replay::state_at(events, t_ms)->FsState`
  (replay.rs:51), `replay_view::children_at(state,dir)->Vec<ReplayEntry>` (replay_view.rs:49),
  `replay_transport::{step_next,step_prev,advance}` (replay_transport.rs:17-66), `replay_session::load_replay(
  base,session)->ReplayData` (replay_session.rs:35). Commands exposed today: ONLY audit_record/sessions/read
  (lib.rs:2377-2417). No `replay*` module is exposed as a command.
- **Track B (shipped, shallow):** CPE-1094 scrubber over in-memory `agentTimeline` (cap 300, cleared on stop) —
  event LIST + diff, no listing reconstruction, no journal read.

## The 2 real gaps
1. **No writer feeds the journal** — `auditRecord` is never called; live `fs-activity` in `flush_fs_batch`
   (lib.rs:4569-4628) emits to UI but never persists. → no durable log to replay.
2. **No baseline** — `state_at` only knows paths that had an event; pre-existing untouched files never appear.

## Build (wiring + writer + baseline + UI — NOT new algorithms)
1. **Persistence = reuse audit_journal** (don't build new). Add `actor: Option<String>` to `AuditEvent`
   (serde default+skip_if_none, backward-compat). Add `record_many(base,&[AuditEvent],max)`. **Writer host-side
   in `flush_fs_batch`** (lib.rs:4625, where actor/kind/path/session_id are in hand) — stamp `ts=epoch_ms`,
   append the batch via `audit_dir(app)`. Host-side = zero IPC chatter (a 50-file batch would be 50 invokes
   frontend-side). Off-means-off: `flush_fs_batch` only runs on the pump thread from `agent_watch_start`
   (lib.rs:4660); no watch → no pump → no append → no `audit/` dir. Reads (`fs-read:` bridge lib.rs:4266) are
   a no-op for listing (state_at treats "read" as no-op) — journal them only if replay should show reads.
2. **Baseline at watch-start** (`agent_watch_start` lib.rs:4661, after `arm`): walk the watched dir once
   (reuse `cpe_server::listing`, bounded like list_dir_stream). **Product micro-decision (A vs B):** (A)
   baseline-as-synthetic-`created`-events into the journal (zero fold logic but pollutes summaries — mitigate
   with a `kind:"baseline"` arm) vs **(B, recommended)** separate `<session>.baseline` snapshot + a ~4-line
   `state_at_from(base,events,t)` seeding the fold from the baseline (keeps the activity log clean). **Scope
   micro-decision:** recursive walk vs **lazy per-folder** immediate-children snapshot (bounds upfront cost;
   matches children_at granularity) — recommend lazy.
3. **`replay_load(session)->ReplayData` command** — thin spawn_blocking into `replay_session::load_replay(
   &audit_dir(app),&session)` (mirror audit_read shell lib.rs:2407). PULL-ONLY (called on Replay-tab open) —
   nothing runs while closed, so replay is zero-cost off with no listener/timer teardown needed.
4. **Frontend:** port the fold into a pure `src/lib/replayFold.ts` (mirror agentReplay.ts, unit-test against
   the Rust `state_at`/`children_at` as oracle) so scrubbing re-derives listing per tick without IPC. In
   `AgentTimeline.svelte` Replay tab: on tab-enter call `replay_load(sessionId)` into `replayEvents`+`baseline`;
   `$: replayListing = childrenAt(stateAtFrom(baseline, replayEvents, t), currentPath)`; render as a read-only
   row list. Transport (t/play/step/slider) + variable-speed (CPE-1104) compose FREE (they only move `t`).

## Slices (order: durable log first)
- **CPE-728a** journal writer (add `actor` + `record_many`; persist in flush_fs_batch). Lowest risk, unblocks all. *(touches bindings: AuditEvent is in bindings.gen.ts via audit_read.)*
- **CPE-728b** baseline snapshot at watch-start (pick A/B + scope).
- **CPE-728c** `replay_load` command + `state_at_from` + `replayFold.ts` (tested vs Rust oracle).
- **CPE-728d** reconstructed listing in the Replay tab (in-drawer; supersedes 300-cap for replay). Medium risk.
- **CPE-728e** file-pane read-only overlay while scrubbing (graduate). Highest risk (coexist with live session;
  strictly read-only + ephemeral + Replay-mode gated + restore-on-exit).

## Coordination with CPE-731 (cost history)
728 reuses the audit journal (per-EVENT). 731 gets a SIBLING `metrics_journal` (per-SESSION row) under the same
app-data root — NOT the same file (grain mismatch). Adding `actor` to AuditEvent (728a) also benefits attribution.
Agree the session-identity schema once across both.

## Off-means-off / no-new-deps
No new deps (all in-tree std-only tested modules + notify/serde). Cost added at 3 points only, all inside the
watching/viewing envelope: 1 append/batch in flush_fs_batch (pump thread), 1 baseline at watch-start (lazy),
on-demand fold when Replay tab open (pull-only). No armed session → no pump → nothing. Replay pull-only → drawer
closed = nothing running.

## Critical files
`src-tauri/src/lib.rs` (writer flush_fs_batch:4569; baseline agent_watch_start:4639; replay_load beside audit
cmds :2377-2417); `crates/server/src/audit_journal.rs` (add actor:21; record_many); `replay.rs` (state_at:51;
add state_at_from); `replay_session.rs` (load_replay:35); `replay_view.rs` (children_at:49);
`src/lib/components/AgentTimeline.svelte` (Replay tab :275-371) + new `src/lib/replayFold.ts` + regen bindings.
