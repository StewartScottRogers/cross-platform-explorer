---
id: CPE-794
title: Folder watcher + action executor (watched-folder rules)
type: feature
status: Deferred
priority: medium
component: Multiple
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-734
estimate: 4h+
---

## Summary
Backend for epic CPE-734: a `notify`-based watcher on user-chosen folders that, when a file lands, runs the
CPE-793 plan through the existing move/copy/tag/rename primitives, logging each action. Opt-in; reversible via undo.

## Acceptance Criteria
- [~] A watched folder fires on new/changed files; the plan executes via existing primitives; actions logged.
      *(**executor landed & cargo-tested** — `run_watch_actions` runs the CPE-793 resolved plan (move/copy/rename) via the shared FS primitives; the live `notify` firing + logging is the integration tail.)*
- [~] Opt-in (nothing watches unless configured); loop/oscillation guarded; actions reversible where possible.
      *(executor is inert until called (opt-in by construction); per-action `OpResult` never all-or-nothing; **oscillation guard + reversibility log** belong with the live watcher — deferred.)*
- [x] cargo/CI green.

## Notes
Prereq: CPE-793. Runs while the app is open (v1). Reuse the FS watcher (Agent Watch) + move/tag commands.

## Work Log
- 2026-07-20 (nightshift, backend authorized) — Picked up. The action model (`watchRules.Action`) is
  constrained to **move / copy / rename / tag** — no external-command execution — so the executor is
  deterministic filesystem work, safely cargo-testable. Grep-first found reusable move/copy/rename impls.
- 2026-07-20 — Refactored the shared copy/move-into-folder logic out of `copy_entries_impl` /
  `move_entries_impl` into `do_copy_into` / `do_move_into` (returning the path actually written, which the
  executor needs); the two bulk commands now call them (dedup; existing move/copy tests still pass).
- 2026-07-20 — Added `run_watch_actions(path, actions)` (+ `_impl`, `WatchAction{kind,resolved}`),
  registered in `generate_handler!`. It runs the frontend planner's resolved pipeline over the landed file:
  `move`/`copy` via `do_move_into`/`do_copy_into`, `rename` via `rename_entry_impl`; the pipeline **threads
  the working path** (a move/rename relocates it for later steps; a copy leaves the original), and each step
  yields a per-action `OpResult` (unknown kind → error, pipeline continues). `tag` is app metadata applied
  via the existing tag store, not here.
- 2026-07-20 — 3 cargo tests (move/copy/rename over a landed file; rename→move pipeline threading;
  unknown-action per-step error without aborting). Full backend suite **156 passed**; clippy clean in BOTH
  feature modes.
- 2026-07-20 — **Deferred.** The executor (AC1's "plan executes via existing primitives" + AC3) is done and
  CI-green. Remaining: the **live `notify` watcher** that fires it on file events — hook into the Agent
  Watch FS watcher, debounce, an **oscillation guard** (don't re-trigger on the moves the executor itself
  makes), an action **log** for review/undo (reversibility), and opt-in config of *which* folders/rules are
  active (pairs with the CPE-795 editor).
  - *deferred-on:* the live-watcher integration + its oscillation guard/log, which pair with the Agent Watch
    event stream and the CPE-795 config GUI. Best built + verified attended (it needs the app running).
  - *revisit-when:* wiring the watcher — subscribe to Agent Watch events for configured folders, run
    `planForEntry` → `run_watch_actions`, guard against self-triggered events, and log for undo. No external
    gate.

## Resolution (partial — executor landed, live watcher deferred)
Landed the deterministic watched-folder **action executor** in `src-tauri/src/lib.rs`:
`run_watch_actions(path, actions)` runs the CPE-793 planner's resolved move/copy/rename pipeline over a
landed file via the shared `do_move_into`/`do_copy_into`/`rename_entry_impl` primitives (also factored out
of the bulk copy/move commands, deduping them), threading the working path across steps and returning a
per-action `OpResult`. 3 new cargo tests; full backend suite 156 green; clippy clean both modes. The live
`notify` watcher that fires it (with oscillation guarding, an undo log, and opt-in folder config) is the
integration tail, deferred to attended work alongside the CPE-795 editor.
