---
id: CPE-548
title: "Busy cursor — shared busy-tracking invoke wrapper (the global boundary)"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-547
estimate: 1-2h
created: 2026-07-16
closed:
---

## Summary
Wave 1 of [[CPE-547]]. The busy-cursor **primitive** already exists (`src/lib/busy.ts`, CPE-482:
ref-counted, 150 ms debounce, toggles `body.busy` → `cursor: progress` via `app.css:500`), but only
3 call sites use it — the ~20 other `invoke(...)` sites import `invoke` **raw** from
`@tauri-apps/api/core`, so a slow call there gives no signal. This ticket builds the shared boundary the
epic's "everywhere" goal needs: one module that wraps core `invoke` in `withBusy`, so a call site gets
the busy cursor **for free** simply by importing from the wrapper instead of the raw package.

## Design (from the epic Decisions)
- **Global wrapper, not per-call opt-in.** New module `src/lib/invoke.ts` exports `invoke` =
  `busy`-wrapped core invoke. Signature is a drop-in for `@tauri-apps/api/core`'s `invoke` so migration
  (CPE-549) is a pure import swap.
- **Escape hatch built in.** The same module re-exports the untracked core invoke as `rawInvoke` for
  streaming / self-progress call sites that must opt out (used by [[CPE-550]]).
- **One threshold app-wide** — reuse `busy.ts`'s `SHOW_AFTER_MS` (150 ms); no per-op tuning.
- Purely frontend; the sidecar/host don't participate.

## Acceptance Criteria
- [ ] `src/lib/invoke.ts` exports `invoke(cmd, args?)` that wraps `@tauri-apps/api/core` invoke in
      `withBusy`, returning the same promise/value and propagating errors unchanged.
- [ ] The busy guard is entered before the call and released on **both** resolve and reject (no leak on
      error).
- [ ] `rawInvoke` is exported as the untracked core invoke for opt-out call sites.
- [ ] Unit tests (jsdom/vitest, mocking core invoke): busy begins around a pending call, clears on
      resolve, clears on reject, and passes args/return through unchanged.
- [ ] `npm run check` clean; no behavior change at existing call sites (they still import raw until
      CPE-549 migrates them).

## Notes
Foundation for [[CPE-549]] (coverage sweep) and [[CPE-550]] (opt-out audit). Keep the wrapper a thin,
predictable pass-through — the PURPOSE tiebreaker (fast/small/predictable) means zero added latency on
the common path beyond the already-debounced cursor.
