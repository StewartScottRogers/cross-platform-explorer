---
id: CPE-1126
title: "Checkpoint & rollback: restore panel + timeline checkpoint markers (GUI cap)"
type: feature
component: Frontend
priority: medium
status: Doing
tags: needs-gui-verify
created: 2026-07-26
epic: CPE-732
---

## Summary
CPE-732's **attended GUI cap** (~15% of the epic). The visual restore experience: a **restore panel** that shows
a checkpoint's revert plan + drift warning for a person to review before reverting, and **checkpoint markers** on
the Agent-Watch timeline. The command-level flow (create/list/preview/revert) ships headlessly via CPE-1125's
palette; this ticket is the visual review layer that genuinely needs human eyes.

## Why Deferred
Building it is fine headlessly, but its VALUE (does the plan read clearly? do markers land right? is the
revert-confirm UX safe?) can only be verified with the user present on the installed build (build → deploy → run,
with a real watched session + checkpoints). Per the workshift skip-and-note escalation, this is deferred to a
GUI-verification session rather than faked. It is on the QA Manual-Verification-Debt ledger.

## Acceptance Criteria (when picked up with the user present)
- [x] A restore panel renders the CPE-1123 `checkpoint_preview_revert` plan + drift warning; confirm-to-revert is
      safe/clear; timeline shows checkpoint markers. Theme vars only; reflow; off-means-off.

## Work Log
2026-07-26 (workshift) — Filed as the CPE-732 deferred GUI cap (PM analysis). Backend + palette + e2e tests ship
headlessly this shift (CPE-1123/1124/1125); this visual layer waits for a user-present GUI session.

2026-07-30 (workshift, Worker) — Built the visual restore layer on the Agent-Watch drawer's Replay tab.
Branch `cpe-1126-checkpoint-markers`, PR against main. Frontend-only; no Rust/specta struct touched (only
consumes existing `commands.checkpoint*` bindings), so no `bindings.gen.ts` regen.

P1 delivered:
- **Pure marker helper + tests (first).** Added `checkpointMarkers(range, checkpoints)` to
  `src/lib/agentReplay.ts` (co-located with `sliderRange`/`sliderFraction`), returning
  `{ cp, fraction, inRange }[]`. Same 0..1 math as `sliderFraction`; handles `range===null` (→ empty) and
  degenerate `firstAt===lastAt` (→ all `fraction:0`, no divide-by-zero). **Clamp decision:** out-of-range
  checkpoints (before `firstAt` / after `lastAt`) are CLAMPED to the nearest track edge (0 or 1) with
  `inRange:false`, so an early/late checkpoint pins to the edge rather than overflowing the track — matches
  the ticket's spec. 9 new unit tests in `agentReplay.test.ts` (empty, null range, no checkpoints, in-range
  positions, exact endpoints, before/after clamping, zero-width range, input-order preservation).
- **Markers on the scrubber.** Wrapped `.rp-slider` in a positioned `.rp-track`; overlay `<button>` pins at
  `left: fraction*100%`, each with a `title` (label/short id + `toLocaleString()`); out-of-range pins get a
  distinct hollow/muted style + a "outside the recorded window" tooltip hint. Click → `stopPlaying()`, sets
  `t = cp.ts` (jumps the scrubber), and selects the checkpoint for the restore panel.
- **Compact restore panel.** On selection, calls `checkpointPreviewRevert(currentPath, manifest_id)` and
  renders the plan exactly like CheckpointDialog: a reflowing counts row (creates/overwrites/deletes/bytes
  via the already-imported `formatBytes`) + a prominent drift warning listing `drift_paths` when
  `drift_count > 0`. "Revert to this checkpoint…" arms the SAME two-step confirm (Cancel + "Yes, revert" →
  `checkpointRevert`), reusing CheckpointDialog's red treatment. Loading/error/outcome states included; on a
  successful revert the list + preview refresh.
- **Root + lifecycle.** Uses the drawer's `currentPath` as the root; empty → nothing loads (no crash).
  Checkpoints load PULL-ONLY on Replay-tab enter (and when the folder changes while on the tab), via a
  `checkpointGen` gen-token guard mirroring `loadReplayData`. off-means-off: markers + panel + loaded list
  fully clear when the tab is left, the session/agent changes (`resetReplay`), or the drawer is destroyed
  (`onDestroy`). Defensive `Array.isArray(res.data) ? … : []` so a null/odd backend payload can't crash the
  marker math.

**Assumption logged:** an out-of-range checkpoint's `t = cp.ts` gets snapped back into the scrubber span by
the existing range-clamp reactive (the slider can only represent in-range moments); the restore panel still
shows the true checkpoint regardless. Acceptable for the edge case; the marker is flagged out-of-range.

P2 (gui-smoke marker snapshot) — **DEFERRED** (noted per ticket, not rabbit-holed). Two blockers: (1) no
release binary is built in this worktree, so `gui-smoke` can't run at all (tauri-driver/msedgedriver are
present in `~/.cargo/bin`, but the app exe is not); (2) seeding a checkpoint isn't cheap — checkpoints come
from real CPE-1123 snapshot on-disk storage (`checkpoint_list` reads the snapshot index), which is a
*different* on-disk shape than the journal+baseline fixture `wdio.conf.ts#seedReplayFixture` writes for the
Replay reconstruction. A future GUI-verify session should either add a `checkpoint_create` test-mode seam (or
have `seedReplayFixture` also write a snapshot index) and then extend `replay.smoke.ts` with
`snap("replay-tab")` (markers visible) + `snap("checkpoint-restore-panel")`.

Verification (all from the worktree):
- `npm run check` → **0 errors, 0 warnings**.
- `npx vitest run src/lib/agentReplay.test.ts` → **36 passed** (was 27; +9 new `checkpointMarkers` tests).
- `npx vitest run` (full suite) → **1410 passed / 126 files, 0 failed.** (Two existing tests needed
  adjusting for the new legitimate `checkpoint_list` call on Replay-tab enter: added an `Array.isArray`
  guard that also fixed `App.replayGuards.test.ts`, and changed `AgentTimeline.test.ts`'s bare
  total-invoke-count assertion to count `replay_load` calls specifically.)
- Confirmed no `#[derive(specta::Type)]` / Rust struct touched — frontend-only diff, no `bindings.gen.ts`
  regen required.
