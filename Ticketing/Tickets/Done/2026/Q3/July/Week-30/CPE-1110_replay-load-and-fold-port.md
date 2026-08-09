---
id: CPE-1110
title: "Activity replay: replay_load command + TS reconstruction fold"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-728
---

## Summary
CPE-728 slice **c**. Expose the persisted replay data to the frontend and port the reconstruction fold to TS
so scrubbing re-derives the folder listing per tick WITHOUT an IPC round-trip each tick. Builds on CPE-1108
(journal writer) + CPE-1109 (baseline + `state_at_from`). Design:
`.claude/research-library/entries/activity-replay-event-reconstruction-plan.md` (§3).

## Context (verified — file:line)
- `crates/server/src/replay_session.rs` — `load_replay(base, session) -> ReplayData{events, bounds, summary}`
  (:35). `crates/server/src/replay_baseline.rs` — `read_baseline(base, session) -> Option<Baseline>` (CPE-1109).
- `crates/server/src/replay.rs` — `state_at_from(base, events, t)` (CPE-1109), `FsState`; `replay_view::children_at`
  (:49) → `Vec<ReplayEntry{name,path,ts,kind}>`.
- `src-tauri/src/lib.rs` — audit commands `audit_read` etc. (~:2377-2417) show the thin `spawn_blocking` +
  `audit_dir(app)` pattern to mirror.
- `src/lib/agentReplay.ts` — the existing pure-frontend replay helpers (dependency-free) — mirror its style for
  the new `replayFold.ts`.

## Design (buildable)
1. **`replay_load(session: String) -> Result<ReplayLoad, String>` command** in `src-tauri/src/lib.rs`
   (thin `spawn_blocking`, mirror `audit_read`): returns `{ replay: ReplayData (load_replay), baseline:
   Option<Baseline> (read_baseline) }` for the session from `audit_dir(app)`. Register in `generate_handler!`
   + the specta list; regen bindings so `replayLoad` + `ReplayData`/`ReplayEntry`/`Baseline` (+ nested) appear.
   Ensure the crossing structs have `#[cfg_attr(feature="specta", derive(specta::Type))]`.
2. **`src/lib/replayFold.ts`** — a PURE, dependency-free port of the reconstruction: `stateAtFrom(baseline,
   events, tMs) -> FsState` (seed from baseline paths, fold events with `ts<=t` in ts order — mirror
   `replay.rs` `fold`/`state_at_from` exactly) + `childrenAt(state, dir) -> ReplayEntry[]` (mirror
   `replay_view::children_at`). **Unit-tested against the Rust behaviour as the oracle** — replicate the Rust
   tests' scenarios (baseline-only, +delete, +create, +rename, +modify, ordering, children projection) so TS
   and Rust agree. Division/empty/NaN-safe; no new deps.
3. **No UI yet** — 728d wires `replayFold.ts` + `replay_load` into the Replay tab. This slice is the command +
   the tested fold only. (Optionally add a tiny doc caveat to `replay_baseline.rs`: the symlink-cycle depth cap
   is defense-in-depth — `metadata()` doesn't traverse symlinks — per the CPE-1109 UAT note.)

## ⚠ Guardrails
- Off-means-off: `replay_load` is PULL-ONLY (called on Replay-tab open in 728d) — nothing runs while closed; no
  listener/timer. No new deps. `replayFold.ts` PURE (no Svelte, no invoke) so it's testable + reusable.
- The TS fold MUST match the Rust `state_at`/`state_at_from`/`children_at` semantics exactly (kind handling:
  created/modified/removed/renamed/read-as-noop; ts ordering; dir vs file). Port faithfully + test against the
  same scenarios.
- Bindings regen + drift-guard must pass.

## Acceptance Criteria
- [ ] `replay_load(session)` returns the session's events+bounds+summary + baseline; registered + bindings
      regenerated (`replayLoad`, `ReplayData`, `ReplayEntry`, `Baseline`); drift-guard passes; `cargo build`
      (app) succeeds.
- [ ] `src/lib/replayFold.ts` `stateAtFrom` + `childrenAt` reproduce the Rust reconstruction (baseline-only,
      +delete/create/rename/modify, ordering, children projection) — vitest-covered, matching the Rust oracle
      scenarios.
- [ ] `cargo test -p cpe-server` green; clippy clean (default + `--features index` + sidecar-platform);
      `npm run check` clean; `npm test` green; no new deps.

## Work Log
2026-07-26 (sprint) — CPE-728 slice c, from the filed plan. Command + TS fold port (tested vs Rust oracle);
728d then renders `childrenAt(stateAtFrom(baseline, events, t), currentPath)` in the Replay tab.
