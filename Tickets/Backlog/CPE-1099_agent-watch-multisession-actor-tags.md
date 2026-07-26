---
id: CPE-1099
title: "Agent Watch: multi-session watching + per-event actor tags (conflict-radar enablement)"
type: feature
component: Backend
priority: low
status: Backlog
tags: big-design
created: 2026-07-26
epic: CPE-396
---

## Summary
GUI #3, conflict-radar slice A (the biggest lift — hence `big-design`). Today `AgentWatchState`
(`src-tauri/src/lib.rs:4127`) watches **one folder for one agent at a time** (`agent_watch_start` replaces,
doesn't add), and `fs-activity` batches carry **no actor identity** (`classify_fs_event:4142` sees raw OS
events — a user's own edit is indistinguishable from an agent write). So "two agents touching the same path"
is currently unobservable. This ticket generalises watching to multiple sessions and tags each event with its
originating actor, so a conflict radar (CPE-1100) can render real (not misleading) data. Context + risks:
`.claude/research-library/entries/agent-watch-dashboards-substrate.md`.

## Design (buildable)
1. **Multi-session watch** — change `AgentWatchState` from `Mutex<Option<AgentWatch>>` to a keyed
   `Mutex<HashMap<sessionId, AgentWatch>>`; `agent_watch_start` takes a `session_id` and ADDS (not replaces);
   `agent_watch_stop` takes a `session_id` (or stops all). **Product decision to make + log**: watch every
   running session unconditionally vs only sessions whose cwd overlaps/nests (cheaper). Off-means-off stays
   absolute — zero watcher threads when no session runs.
2. **Per-event actor tag** — add a `sessionId`/actor field to each `fs-activity`/`fs-diff` batch item
   (`flush_fs_batch:4228`). App-initiated operations the app already knows about (rename/move/delete/copy via
   existing commands) get a **"user"** tag; unrelated-process writes inside a watched folder are labelled
   **"unknown actor"**, NOT silently attributed to the agent (avoid false-positive conflicts).
3. **Frontend plumbing** — thread the actor tag through `sidecar.ts`/`agentActivity.ts` so consumers can
   distinguish actors (the radar panel is CPE-1100; this ticket just carries the signal).

## ⚠ Notes / guardrails
- **Off-means-off is the hard constraint**: N watcher threads when N agents run is a NEW cost this mode hasn't
  paid — it must still collapse to zero when nothing runs; log the watch-all-vs-overlap decision. No new deps.
  Async + spawn_blocking for watcher setup. Event-driven.
- Do NOT ship a radar on single-agent data — this enablement is the prerequisite that makes the radar honest.

## Acceptance Criteria
- [ ] Multiple running sessions are watched concurrently (keyed map); each fs-activity/fs-diff event carries an
      actor tag (agent session id / "user" / "unknown"); app-initiated ops tag as "user".
- [ ] Zero watcher threads / listeners when no session runs; the watch-all-vs-overlap choice is implemented and
      logged in the work log with rationale.
- [ ] clippy clean (default + `--features index` + sidecar-platform); existing agent-watch tests still green;
      `npm run check` clean; no new deps.

## Work Log
2026-07-26 (workshift, GUI) — Filed as GUI #3 conflict-radar ENABLEMENT (the architecture change) from the
Library substrate brief. `big-design` — largest of the GUI #3 slices; do LAST. Blocks CPE-1100. May warrant a
Researcher/Plan pass at activation given the off-mode-leanness + actor-attribution subtleties.

## Build spec (de-risked, 2026-07-26)
A Plan/architect pass produced a full build spec — filed at
`.claude/research-library/entries/agent-watch-multisession-actor-plan.md` (READ IT before building). Key
outcomes: **product decision = WATCH ALL running sessions** (overlap detection is the radar's frontend fold,
not a watch-selection concern; cost = 2N threads for N sessions, collapses to 0 when idle). **Split this ticket
into sub-slices at build time:** (a) multi-session watch structural refactor — keyed
`HashMap<sessionId,AgentWatch>`, ADD-not-replace `agent_watch_start(session_id,path)`, `agent_watch_stop(id)`
+ `agent_watch_stop_all`, frontend desired-set−armed-set reconcile with ONE shared listener pair; (b) actor
tags — `actor` field (sessionId/"user"/"unknown") via a no-thread app-op ledger + `note_app_op` at 8 file-op
sites + `sessionId` on `read_announcement`; (c) optional honest unknown-vs-agent via sidecar `fs-write:`
reporting. Slice (a) is highest-risk (edits the #413 lib.rs region + off-means-off across session churn) — do
it first behind an unchanged event shape. Build AFTER #413 merges.
