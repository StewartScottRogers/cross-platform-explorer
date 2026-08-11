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
