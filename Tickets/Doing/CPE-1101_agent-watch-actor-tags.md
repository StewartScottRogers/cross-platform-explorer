---
id: CPE-1101
title: "Agent Watch: per-event actor tags (conflict-radar enablement, slice b)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-396
depends-on: CPE-1099
---

## Summary
GUI #3 conflict-radar **slice (b)** — add a per-event **actor** tag (the owning `sessionId`, `"user"`, or
`"unknown"`) to every `fs-activity`/`fs-diff` item, so the radar panel (CPE-1100) can fold same-path writes
across *distinct* actors into an overlap/conflict signal. Depends on CPE-1099 (multi-session watch). Full
design: `.claude/research-library/entries/agent-watch-multisession-actor-plan.md`.

## Design (buildable — from the filed plan)
1. **Wire shape** — `flush_fs_batch` (src-tauri/src/lib.rs) adds `"actor"` to each activity item
   (`{kind, path, actor}`) and each diff item (`{path, before, after, actor}`). Default `actor = <owning
   sessionId>` (the pump owns its session id — thread `session_id` into `fs_activity_pump`/`flush_fs_batch`).
2. **`fs-read:` actor** — `read_announcement` (sidecar `console.rs:177`) adds `sessionId`; the host `fs-read:`
   arm sets `actor: sessionId` on its emitted `{kind:"read", path, actor}`.
3. **"user" tag via a no-thread app-op ledger** — `AgentWatchState.app_ops:
   Mutex<VecDeque<(normalizedPath, Instant)>>`. A `note_app_op(app, &[paths])` records target paths *before*
   the mutation at the 8 file-op `*_impl` sites (rename/delete/copy/move/move_exact/create_dir/create_file/
   write_file_text). `flush_fs_batch` checks each event's path against the ledger; a fresh (<~2s) match →
   `actor = "user"` (consume it). **No thread, no timer** — entries age out by `Instant` comparison during
   flush. `note_app_op` is a **no-op without `sidecar-platform`** (`#[cfg(not(...))]` empty body) so the plain
   explorer compiles + pays nothing; under the feature it grabs `app.try_state::<AgentWatchState>()`.
4. **"unknown"** — events under a watched folder whose owning session already ended (pump-drain race) tag
   `"unknown"` rather than a dead session id.
5. **Frontend threading** — `sidecar.ts FsActivity` (+ `normalizeFsActivity`, default `"unknown"` if absent) /
   `agentActivity.ts` (`AgentActivity`/`TimelineEntry`) / `agentDiffs.ts` (`FsDiff`) carry `actor`. Existing
   consumers ignore it. Extend `sidecar.test.ts`/`agentActivity.test.ts`/`agentDiffs.test.ts`.

## ⚠ Notes / guardrails
- The app-op ledger adds **no thread/timer** (aged in-line) — do not smuggle in background cost. `note_app_op`
  no-op without the feature (plain explorer untouched). No new deps.
- Honesty: default `actor = owning sessionId`; a raw watcher can't prove a write came from the agent vs an
  unrelated process — the *positive* "unknown for unconfirmed writes" upgrade is the optional slice
  CPE-1102 (sidecar `fs-write:` reporting). Until then CPE-1100 hedges its wording.

## Acceptance Criteria
- [ ] Every fs-activity/fs-diff item (incl. `fs-read:`) carries `actor` (sessionId / "user" / "unknown");
      app-initiated ops (the 8 sites) tag "user" via the no-thread ledger; `note_app_op` is a no-op without
      `sidecar-platform` (plain build unaffected — verify it compiles + `cargo test` default features).
- [ ] Frontend stores carry `actor` (default "unknown" if absent — old payloads stay valid); tests extended.
- [ ] clippy clean (default + sidecar-platform); existing agent-watch tests green; `npm run check` clean;
      bindings regen if any typed surface changed; no new deps; no new thread/timer.

## Work Log
2026-07-26 (workshift, GUI) — Filed as conflict-radar slice (b) from the build spec. Follows CPE-1099
(multi-session watch). Blocks CPE-1100 (the radar panel).
