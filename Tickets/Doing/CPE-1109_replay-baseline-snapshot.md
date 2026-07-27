---
id: CPE-1109
title: "Activity replay: baseline snapshot at watch-start + state_at_from"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-728
---

## Summary
CPE-728 slice **b**. The reconstruction fold (`replay::state_at`) only knows paths that had an *event*, so
pre-existing untouched files never appear in a reconstructed listing. Capture a **baseline snapshot** of the
watched directory at watch-start, and add `state_at_from(baseline, events, t)` that seeds the fold from it —
so `children_at(state_at_from(...), dir)` shows the folder as it actually looked. Backend. Builds on CPE-1108
(journal writer). Design: `.claude/research-library/entries/activity-replay-event-reconstruction-plan.md`.

## Decisions (from the plan — decide-and-log, no user input needed)
- **Representation B (separate snapshot)** — write a `<session>.baseline` file (paths of pre-existing entries),
  NOT synthetic events into the activity journal (keeps the event log semantically = agent actions only, so
  `activity_timeline::summarize` isn't polluted).
- **Scope: bounded recursive walk at watch-start** — snapshot the watched root's tree once, reusing the
  existing bounded listing (the caps `list_dir_stream` uses). Log/cap on a huge tree (accept truncation — a
  bounded baseline is the documented limitation; a lazy-per-folder refinement can follow if needed).

## Context (verified — file:line)
- `src-tauri/src/lib.rs` `agent_watch_start` (~:4639-4687) — after `state.arm(...)`, `app` + the watched
  `path` + `session_id` are in hand; `audit_dir(app)` (~:2370) gives the per-session dir root. Baseline lives
  beside the journal.
- `crates/server/src/replay.rs` — `state_at(events,t)->FsState` (:51) folds via private `fold` (:66); `FsState`.
  Add `state_at_from(base:&FsState, events, t)->FsState` (clone base, fold events≤t). `replay_view::children_at`
  (:49) projects.
- `crates/server/src/listing` (used by `list_dir`/`list_dir_stream`, lib.rs:460/489) — reuse for the bounded walk.

## Design (buildable)
1. **Baseline module** (in `crates/server`, e.g. `replay_baseline.rs`): `capture(root) -> Vec<String>` (or a
   `FsState`) — a bounded recursive walk of `root` collecting pre-existing entry paths (dirs+files), reusing
   the listing bounds; `write_baseline(base, session, &Baseline)` + `read_baseline(base, session) -> Option<..>`
   mirroring `audit_journal`'s file IO (JSON/JSONL, sanitized filename, temp-file+rename). Pure walk logic
   unit-tested.
2. **`state_at_from`** in `replay.rs` — `pub fn state_at_from(base:&FsState, events:&[AuditEvent], t_ms:u64)
   -> FsState` (clone base, fold `events` with `ts<=t` in ts order via the existing `fold`). Unit-tested:
   baseline-only (no events) → the baseline listing; baseline + a delete event → path gone; + a create → added.
3. **Capture at watch-start** — in `agent_watch_start`, after arming: `spawn_blocking` a bounded
   `replay_baseline::capture(path)` and `write_baseline(&audit_dir(app), &session_id, &baseline)`. Errors
   logged/swallowed (a baseline failure must not break watching). Off-means-off: only runs at watch-start (no
   watch ⇒ no capture).
4. **Expose read** — a `replay_baseline(session)` will be needed by the load command (728c); either add it here
   or leave the read fn for 728c. If a type crosses the boundary, regen bindings + drift-guard.

## ⚠ Guardrails
- Off-means-off: baseline capture only at watch-start (inside `agent_watch_start`); nothing when idle. No new
  deps (listing + serde + std). Bounded walk (cap huge trees, log truncation). `state_at_from` is pure.
- Baseline is representation B (separate file) — do NOT pollute the activity journal with baseline entries.
- A baseline read/capture error must not break watching or replay (degrade to events-only reconstruction).

## Acceptance Criteria
- [ ] At watch-start a bounded baseline snapshot of the watched root is written per-session (separate file, not
      the activity journal); `state_at_from(baseline, events, t)` reconstructs the listing including pre-existing
      untouched files; a delete/create event applied over the baseline is reflected. `cargo test -p cpe-server`
      green (baseline capture walk bounded; state_at_from: baseline-only, +delete, +create, ordering).
- [ ] Baseline capture/read failure is logged/swallowed (watching + events-only replay still work); nothing is
      captured when not watching.
- [ ] clippy clean (default + `--features index` + sidecar-platform); no new deps; bindings regen + drift-guard
      pass if any type crosses; `npm run check` clean.

## Work Log
2026-07-26 (workshift) — CPE-728 slice b, from the filed plan. Chose representation B (separate snapshot) +
bounded recursive walk (decide-and-log micro-decisions). Unblocks 728c (replay_load + fold port) and 728d
(listing UI).
