---
id: CPE-1643
title: "Two more subscriptions outlive onDestroy — the agent-watch diff/cost listeners and the notice timer"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the worker on CPE-1633 while fixing the smart-folder live-refresh leak. It was asked to scan
`onDestroy` for other subscriptions of the same shape and **report rather than expand its diff** — it found
two and did exactly that.

## The gaps
1. **`unlistenDiffs` / `unlistenCost`** (`src/App.svelte`, armed/disarmed in the agent-watch reconcile block
   around L1476-1492). These are set up alongside `unlistenActivity`, but **only `unlistenActivity` is torn
   down in `onDestroy`**. So on component destroy with a watch still armed, two event listeners are left
   registered.
2. **`noticeTimer`** (`src/App.svelte` ~L2049) — a `setTimeout` that is cleared only when a *new* notice
   replaces it, never in `onDestroy`. A pending notice timer therefore outlives the component.

Both are the identical shape to CPE-1633: a subscription or timer created in one place, correctly released on
its own natural close path, and never released when the component itself goes away.

## Why it's Low, and why it's still worth doing
In the running app these are small, bounded leaks that die with the process. The real cost is **under test**:
a genuine (non-mocked) `setTimeout` or an orphaned listener can fire during whatever test happens to be
running afterwards in the same process, reaching into a destroyed component's closure. That is precisely the
class of contention-dependent flake that made CPE-1628 unreproducible and cost a worker a full investigation
before it was deferred.

Note the honest caveat recorded on CPE-1633: fixing one leak of this shape did **not** visibly reduce the
single-fork stress failures. So don't expect this ticket alone to fix the flakes either — the value is
removing a class of nondeterminism, not a guaranteed green.

## Fix
Add the missing teardowns to `onDestroy` alongside the existing ones. Then — the part that stops this
recurring — **sweep the component for the whole class rather than these two instances**: every
`setTimeout`/`setInterval` handle and every `unlisten`/subscription stored in a module- or component-level
variable should be released in `onDestroy`. List what you find, and say whether a lint rule or a small guard
test could make the next omission fail rather than be discovered by a stress run months later.

## Acceptance criteria
- Both named leaks are torn down in `onDestroy`.
- A test per leak that **fails against current code** (negative control — report the observed failures), in
  the style CPE-1633 established: spy on `clearTimeout` / the unlisten mock rather than waiting out a real
  timer. Note CPE-1633's worker hit a subtlety worth reusing — a spy installed *after* `render(App)` misses
  `TrailingDebounce`'s constructor-default-param capture of `setTimeout`.
- The sweep's findings are recorded, even where not fixed.
- No behavioural change beyond teardown.

**Conflict surface:** `src/App.svelte` and its test files. `App.svelte` is a hot file — check nothing else is
mid-flight on it before starting.

## Work Log
2026-08-11 — Added both missing teardowns to `onDestroy` (`src/App.svelte:6207-6234`):
`unlistenDiffs?.(); unlistenCost?.();` alongside the existing `unlistenActivity?.();`, and
`if (noticeTimer) clearTimeout(noticeTimer);` at the end of the same block.

**Tests (one file per leak, both with a real negative control)**:
- `src/App.agentWatchListenerTeardown.test.ts` — arms a real agent-watch session (`ingestSessionState`
  "started" inside the current folder), captures per-EVENT-NAME teardown spies from a `listen()` mock
  (stricter than the shared no-op teardown `App.agentWatchPauseMetrics.test.ts` uses — it can tell
  "fs-diff's unlisten ran" apart from "fs-activity's unlisten ran"), destroys the component while the
  watch is still armed, and asserts `ai-console://fs-diff` and `ai-console://agent-cost`'s teardowns were
  both called once — with `ai-console://fs-activity`'s as a sanity control (already worked pre-fix).
- `src/App.noticeTimerTeardown.test.ts` — reuses `App.smartFolderBlockedNotice.test.ts`'s (CPE-1614)
  proven trigger (open a smart folder, press Delete with no selection → `blockedInArchive()` fires
  `showNotice` unconditionally) to arm the 5s notice timer, spies on `window.setTimeout`/`clearTimeout`
  (installed before mount, matching CPE-1633's established pattern), destroys without waiting the 5s out,
  and asserts `clearTimeout` was called with the notice timer's own handle.

**Negative control, actually run (not just claimed)**: temporarily reverted both `onDestroy` additions,
re-ran both files — `App.agentWatchListenerTeardown.test.ts` failed on the `diffTeardowns`/`costTeardowns`
assertions (spy never called) while its `fs-activity` control kept passing;
`App.noticeTimerTeardown.test.ts` failed on `expect(clearTimeoutSpy).toHaveBeenCalledWith(noticeHandle)`
(0 matching calls, dumped a live un-cleared `Timeout` object). Re-applied the fix and confirmed both pass
again. This is a real, executed negative control per the acceptance criteria, not an inferred one.

**Debugging note for future reference**: my first attempt put both tests in ONE file sharing a single-drive
fixture (`drives: [{ path: "C:\\proj" }]`); the notice-timer test intermittently failed because opening a
smart folder against a fixture with exactly one real drive (non-empty `list_dir`/`list_dir_stream`) let
some other reactive step revert `smartFolder` back toward Home between the "opened" assertion and the
`Delete` keydown, so the notice never fired — never root-caused (out of scope to chase), just sidestepped
by giving the notice-timer test its own file with the EXACT drive/listing fixture the already-proven
CPE-1614 test uses (`"Local Disk (C:)"`, always-empty `list_dir`). Splitting also matches this repo's own
established convention (see `App.watchLiveGate.test.ts`'s docstring) of isolating tests that could
otherwise share module-singleton-store state.

**Sweep of the whole class** (every `setTimeout`/`setInterval` handle and every `unlisten`/subscription
stored in a module- or component-level variable in `src/App.svelte`), findings recorded per the ticket's
ask even where not fixed:
- Fixed this ticket: `unlistenDiffs`, `unlistenCost`, `noticeTimer`.
- Already correctly torn down (pre-existing, verified by reading `onDestroy`): `verifyTimer`
  (`clearInterval`), `smartRefreshDebounce`/`smartRefreshUnlisten` (CPE-1633), `unlistenSessions`,
  `unlistenTransferDone`, `unlistenOpenDocs`, `unlistenSpotlightOpen`, `unlistenTrayOpen`, `unlistenOsDrop`,
  `unlistenActivity`, `watchRefreshTimer` (`clearTimeout`), `autoMirrorTimer` (`clearInterval`), plus the
  three `window.removeEventListener` pairs and `stopDriveScheduler()`/`stopDriveWatch()`.
- **Found, NOT fixed (out of this ticket's two-named-leaks scope, recorded per the ticket's ask)**: two
  `setTimeout` calls whose handle is never captured in any variable, so they can't be cancelled even if
  someone wanted to:
  1. `App.svelte` ~L5731 (`popOutPreview`) — `setTimeout(finish, 2500)`, a fallback so a slow pop-out
     window load never hangs. Lower risk: purely local to the async function, resolves a local `Promise`
     guarded by a `done` flag, touches no store/component state directly — the risk is the *rest* of
     `popOutPreview`'s async continuation running after destroy, not this timer specifically.
  2. `App.svelte` ~L5983 (`onMount`, opt-in startup integrity check) —
     `setTimeout(() => { void verifyAllBaselines(); }, 1500)`. Real, if narrow, risk: a mount-then-
     immediate-unmount within 1.5s (plausible in a fast test run) lets this fire after destroy and call
     into `verifyAllBaselines()`, which touches component state. Recommend a follow-up ticket to capture
     the handle in a component-level variable and clear it in `onDestroy`, alongside `verifyTimer`.
- **Lint rule vs. guard test**: a generic ESLint rule for "every `setTimeout` return value must be stored
  and later `clearTimeout`'d" would false-positive heavily in this codebase (many fire-and-forget UI
  timers are genuinely fine to let run to completion, e.g. debounce-adjacent one-shots with no store
  writes) — not worth the noise. A **targeted guard test** is the better fit and is exactly what this
  ticket's own two tests demonstrate as a reusable pattern: spy on `setTimeout`/`clearTimeout` (or the
  `listen` mock's per-registration teardown) around a specific destroy-while-armed scenario. Recommend
  documenting that pattern (arm → destroy → assert every captured handle/teardown was released) as the
  house convention for any NEW `let x: (() => void) | ReturnType<typeof setTimeout> | null` added to
  `App.svelte`, rather than trying to lint the general case.

**Verification**: `npm run check` (0 errors/0 warnings); full `npx vitest run` (289 files, 3661 tests, all
green, no regressions vs. baseline ~3660).

**No cargo changes** — this ticket is frontend-only; no Rust touched, no bindings regenerated.
