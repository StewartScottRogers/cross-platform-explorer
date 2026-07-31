---
id: CPE-1152
title: "gui-smoke: seed a checkpoint so the restore panel + markers can be screenshotted (Visual-Critic verify)"
type: chore
component: Testing
priority: medium
status: Done
tags: ready
created: 2026-07-30
closed: 2026-07-30
epic: CPE-579
---

## Summary
The Agent-Watch **restore panel + checkpoint markers** (CPE-1126) and their refinements (CPE-1151) are
code-complete, reviewed, and unit/component-tested — but they have **never been seen in a screenshot**, so the
new Visual Critic can't judge the look and the user has to be pulled in. The blocker: gui-smoke's
`seedReplayFixture` writes the replay **journal + baseline** (which makes the scrubber render), but a
checkpoint **marker** needs a real CPE-1123 **snapshot store** entry (`checkpoint_list` reads the snapshot
index), which is a *different* on-disk shape the fixture doesn't create. So the Replay tab renders with a
scrubber but zero markers, and the restore panel is unreachable in the harness.

Close that gap so one `npm test` run captures the restore panel — completing CPE-1148's screenshot loop for
this surface and retiring its Manual-Verification-Debt row.

## Acceptance Criteria
- [x] gui-smoke can seed at least one **checkpoint** for the same folder + within the seeded replay session's
      time range, so a **marker** renders on the Replay scrubber. Preferred mechanism (worker's judgment):
      drive the app's real `checkpoint_create` via the running webview (a `browser.execute` tauri invoke) so
      it writes the genuine snapshot store; OR extend `wdio.conf.ts#seedReplayFixture` to also write a minimal
      valid snapshot index for the folder. Whatever is chosen, use the REAL read path (`checkpoint_list`) — no
      faked marker.
      → Chose mechanism (a): `checkpoint-restore.smoke.ts` drives the genuine `checkpoint_create` via a
      `browser.executeAsync` `window.__TAURI_INTERNALS__.invoke`, keyed by `window.__CPE_OPEN_DIR__` (byte-
      identical to the frontend's `currentPath`), so the REAL `checkpoint_list` reads it back — no faked marker.
      Two live-timeline entries seeded at `ts ± 30s` bracket the checkpoint so the marker pins mid-track (`inRange`).
- [x] A spec (extend `replay.smoke.ts` or a new `checkpoint-restore.smoke.ts`) drives: open Agent Watch →
      Replay tab → assert a marker is present → click it → assert the restore panel renders the preview
      (counts + drift) → `snap("checkpoint-restore-panel")`; and if a drifted file is staged, arm the confirm
      and `snap("checkpoint-revert-confirm")` so the Visual Critic can see the drift echo. Keep specs
      non-blocking (`continue-on-error`) and their existing assertions intact.
      → New `gui-smoke/specs/checkpoint-restore.smoke.ts` does exactly this; a drifted file (overwrite) is
      staged so `checkpoint-confirm-drift` is captured. Existing specs untouched.
- [x] Running the real harness produces the new PNG(s) in `.screenshots/`; `npm run check` green; the new
      spec passes (or fails only on the same pre-existing env-drift the other specs do).
      → `checkpoint-restore-panel.png` + `checkpoint-revert-confirm.png` produced; `npm run check` 0/0;
      gui-smoke `typecheck` clean; spec PASSES against a fresh binary.

## Work Log
- 2026-07-30 — Built `gui-smoke/specs/checkpoint-restore.smoke.ts` (test-infra only; no app-code change).
  - **Seed mechanism (a):** the spec calls the app's genuine `checkpoint_create` command from the running
    webview (`browser.executeAsync` → `window.__TAURI_INTERNALS__.invoke("checkpoint_create", {root,label})`),
    using `window.__CPE_OPEN_DIR__` as `root`. Because `App.svelte` sets `currentPath = navigate(__CPE_OPEN_DIR__)`
    verbatim, that root is byte-identical to the string the frontend's `checkpoint_list(currentPath)` uses, so
    the store's `sha256(root)` key matches and the marker renders via the REAL read path (no faked marker, no
    hand-written on-disk store). The spec never Node-writes app-data, so it's identifier-agnostic (works against
    the base OR the `.sidecar` build). Live-timeline entries are seeded at `ts ± 30s` (via CPE-1135's
    `__CPE_TEST_INGEST_ACTIVITY__`) so the scrubber range brackets the checkpoint and the marker pins mid-track
    (`inRange`, not edge-clamped). A pre-checkpoint file is overwritten post-checkpoint to stage drift.
  - **Flow asserted:** open Agent Watch → Replay tab → checkpoint marker present + in-range → click →
    `checkpoint-restore-panel` + `checkpoint-counts` + `checkpoint-drift-warning` render → `snap` → arm revert →
    `checkpoint-confirm-revert` + `checkpoint-confirm-drift` render → `snap`. Self-cleans the throwaway store
    (`checkpoints/<sha256(root)>`) + drift file in `after()`.
  - **Verify:** `npm run check` → 0 errors / 0 warnings. `cd gui-smoke && npm run typecheck` → clean. Built a
    fresh base binary (`npm run build && npm run tauri build -- --no-bundle`, 2m25s warm) and ran
    `npm test --spec checkpoint-restore.smoke.ts` → **1 passing**. Both `.screenshots/checkpoint-restore-panel.png`
    and `.screenshots/checkpoint-revert-confirm.png` produced and visually confirmed (real restore panel: counts
    creates 0 / overwrites 1 / deletes 0 / 48 B, `drift 1`, drift warning + `CPE-1152-drift.txt`; confirm panel
    echoes "1 file changed since this checkpoint will be lost").
  - **Note (not a bug):** the pre-existing `src-tauri/target/release` binary was built 16:54, *before* CPE-1151
    (dd25e52b, 18:20) added the `checkpoint-confirm-drift` echo, so a first run failed only that final assertion —
    the embedded frontend was stale. Rebuilding from current source (which has CPE-1151) made the spec pass. No
    app/source change was needed; this is purely the "rebuild the binary if unsure it contains CPE-1151" caveat
    the ticket called out.

## Notes
- Epic CPE-579 (self-maintaining quality infra) — the visual rung for CPE-732's restore UI. Unblocks the
  first *formal* Visual-Critic `VISUAL PASS`/`VISUAL CHANGES` verdict on a real GUI ticket.
- Origin: the P2 deferral in CPE-1126 + the user's GUI-verify. Retires the CPE-1126 row on
  `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`.
- Test-infra only; no app-code change expected (consumes existing `checkpoint_create`/`checkpoint_list`).
