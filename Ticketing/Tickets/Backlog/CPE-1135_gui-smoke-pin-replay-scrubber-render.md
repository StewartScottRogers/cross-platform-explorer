---
id: CPE-1135
title: "QA: pin the Agent-Watch replay-scrubber render in gui-smoke (burn down CPE-1094 manual debt)"
type: chore
component: Testing
priority: medium
status: Backlog
tags: ready
created: 2026-07-29
epic: CPE-728
---

## Summary
QA-Architect pass, 2026-07-29. The Agent-Watch **replay scrubber** (`AgentTimeline.svelte` Replay tab,
CPE-1094 / epic CPE-728) is on the Manual-Test-Burndown as "headless UAT+review only — needs eyes on a real
watched session in the installed build." The CPE-1130 work proved the pattern to erase exactly this class of
debt: seed a synthetic watched-session via the `--test-mode`-gated `window.__CPE_TEST_INGEST_SESSION__` hook to
reach the drawer, seed the tab's *data* as an on-disk fixture, drive the real `tauri build` binary, and assert
the tab renders. Apply that pattern to the Replay tab so its **render can never silently regress**.

Same framing as CPE-1114/CPE-1130: this pins the *render* (tab reachable, scrubber transport + slider +
reconstructed listing appear from a real fixture). The *feel* residual (slider-drag latency, play cadence,
diff-on-scrub animation) stays a human-glance item — but the "does the Replay tab render at all" surface stops
being manual.

## What exists (verified 2026-07-29)
- Replay tab lives in `src/lib/components/AgentTimeline.svelte` (`tab: "live" | "replay" | ...`, line ≈ 97).
  Stable render selectors: `.rp-transport`, `.rp-btn`, `.rp-play`, `.rp-slider` (`<input type="range">`,
  line ≈ 550).
- `loadReplayData(session)` (line ≈ 273) calls the `replay_load` command (CPE-1110), which ships the session's
  durable **audit journal + baseline** from disk; scrubbing then folds locally (`replayFold.ts`).
- `gui-smoke/wdio.conf.ts` already seeds a fixture into the REAL app-data dir Tauri reads
  (`seedHistoryFixture()`, line ≈ 211, → `<app-data-dir>/agent-metrics/history.jsonl`) and restores it in
  `onComplete`. `gui-smoke/specs/cost-history.smoke.ts` is the reference spec (drawer reach + tab open +
  assert-render + the `window.__CPE_TEST_INGEST_SESSION__` seed).

## Design (headless; test-only + a possible tiny test-mode seed hook)
- Add `seedReplayFixture()` to `gui-smoke/wdio.conf.ts` (mirror `seedHistoryFixture` — same
  seed-then-restore-in-`onComplete` discipline). It writes the on-disk **audit journal + baseline** for a fixed
  synthetic session id (e.g. `gui-smoke-replay`) into the real app-data dir the built binary reads from.
  **Discover the exact file paths + JSON shapes** from the backend that `replay_load` reads — see
  `crates/server/src/audit_journal.rs` (`session_file`/`read_session`), `replay_baseline.rs`, and
  `replay_session.rs` (`load_replay`) and their tests, which construct these fixtures in-code. Reuse those
  shapes; do not invent a format.
- Add `gui-smoke/specs/replay.smoke.ts`: wait for the `--open` breadcrumb, seed the synthetic session
  (`__CPE_TEST_INGEST_SESSION__`) with `sessionId: "gui-smoke-replay"` and the same `cwd` (so the drawer's
  `.agent-log-btn` appears), open the drawer, click the **Replay** tab, trigger the session load, and assert
  `.rp-transport` + `.rp-slider` render and the reconstructed listing (or an explicit loaded/empty state) is
  present and non-error. Keep it a **non-blocking** CI smoke signal (`continue-on-error`, per CPE-1048), matching
  the other smoke specs.
- If reaching the Replay tab's *loaded* state needs the session id threaded to the load call in a way the
  synthetic-session hook doesn't already cover, add the **minimum** `--test-mode`-gated seam (mirror the
  existing `__CPE_TEST_INGEST_SESSION__` / `__CPE_OPEN_DIR__` conventions — zero surface outside `--test-mode`).
  Prefer asserting whatever render state is reachable without new app code if that already proves the tab renders.

## Acceptance Criteria
- [ ] `gui-smoke/specs/replay.smoke.ts` drives the real built binary to the Agent-Watch drawer's Replay tab and
      asserts the scrubber transport (`.rp-transport`) + slider (`.rp-slider`) render from a seeded replay
      fixture, non-error.
- [ ] `seedReplayFixture()` seeds the audit-journal + baseline fixture into the real app-data dir using the
      backend's actual on-disk shapes, and `onComplete` restores/cleans it (no leftover state, mirroring the
      history fixture).
- [ ] The spec is wired into the `GUI smoke` CI job as a non-blocking signal (`continue-on-error`, CPE-1048),
      alongside the existing smoke specs.
- [ ] Any new test-mode seam (if needed) is gated behind `--test-mode` / `window.__CPE_TEST_MODE__` with no
      effect in production, matching the existing hooks.
- [ ] `npm run check` passes; the existing smoke specs (`open-dir`, `cost-history`) still pass.

## Notes
- Burns down the render portion of Manual-Test-Burndown row **CPE-1094**; flip that row to
  "render pinned by gui-smoke (CPE-1135; non-blocking per CPE-1048); feel residual" on merge and decrement MVD
  accordingly (same treatment as CPE-1114 → CPE-1130).
- This machine HAS tauri-driver + msedgedriver, so the worker CAN run the real gui-smoke locally to prove the
  spec green (~30–40m) — worth it; CPE-1130 caught a real bug that way. See research-library entries
  `gui-smoke-devtoolsactiveport-webview2-ci.md` and `headless-gui-smoke-test-tauri-driver.md` for the WebView2
  runner args if a local run misbehaves.
