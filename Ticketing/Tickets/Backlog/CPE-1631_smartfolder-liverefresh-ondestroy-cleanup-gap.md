---
id: CPE-1631
title: "App.svelte onDestroy never cancels the CPE-1230 smart-folder live-refresh debounce/listener"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found during the CPE-1628 investigation (test-order pollution in `savedSearchStore.test.ts`) while
tracing every real, un-mocked `setTimeout`/listener in `App.svelte` that could survive past a single
test's teardown. This one doesn't explain CPE-1628's specific symptom (it never touches
`cpe.savedSearches`), so it's filed separately rather than folded into that fix — CPE-1628's own scope
note says to file a follow-up rather than widen its diff.

## The gap
`App.svelte`'s CPE-1230 smart-folder live-refresh wiring (around line 2001) declares:

```ts
let smartRefreshUnlisten: (() => void) | null = null;
const smartRefreshDebounce = new TrailingDebounce(300);
```

`manageSmartFolderLiveRefresh` correctly cancels the debounce and unlistens when the open smart
folder/structured search **closes** (`scope` goes `null`) — but `onDestroy` (around line 6171), which
tears down every other subscription (`unlistenSessions`, `unlistenTransferDone`, `verifyTimer`,
`autoMirrorTimer`, the `contextmenu`/`focus` window listeners, drive watchers, …), never calls
`smartRefreshDebounce.cancel()` or `smartRefreshUnlisten?.()`.

Concretely: if the App component is destroyed (real app: window close mid-navigation; tests:
`@testing-library/svelte`'s auto `cleanup()` after `afterEach`) while a structured search or tag smart
folder is **open**, the live `folder-watch` listener registration and/or a pending 300ms debounce timer
are never released. In production this is a small, bounded leak (single listener + single timer, gone
when the process exits). Under test, it's a real correctness gap: a genuine (non-mocked) `setTimeout`
outlives the test that scheduled it and can fire during whatever test happens to be running ~300ms
later, in the same process, calling into a closure over the destroyed component's stale reactive state.

## Evidence it's real, not theoretical
Forcing `npx vitest run --pool=forks --poolOptions.forks.singleFork=true` (the whole 273-file suite in
one OS process — the worst case for exactly this kind of leak) produced intermittent multi-second
timeouts in tests unrelated to the file that scheduled the original timer (observed: a
`Sidebar.hoverSameVolume` race-guard test, an `App.folderPeek` test, and — inside
`App.smartFolderLiveRefresh.test.ts` itself — its own later "tag smart folder live-refresh" test).
These same tests are reliably green under the normal (non-single-fork) default config the project
actually runs, so this is exposure that scales with how squeezed the process/worker scheduling is, not
a today-blocking bug — but CI worker counts and machine load are not something this crew controls, and
"only fails under contention" is exactly the kind of intermittent-red pattern CPE-1628 was filed to stop
tolerating.

## Scope
- Add `smartRefreshDebounce.cancel();` and `smartRefreshUnlisten?.();` to `onDestroy` in `App.svelte`,
  alongside the existing teardown calls.
- A regression test: render `App`, open a structured search (arming the listener / scheduling a
  debounce via a synthetic `folder-watch` event), unmount via `cleanup()` **without** closing the
  search first, then assert the debounce's timer handle was cleared (e.g. by spying on
  `clearTimeout`/the real `@tauri-apps/api/event` unlisten mock) rather than by waiting out a real
  300ms window and hoping nothing fires.
- Do not touch `TrailingDebounce`, `manageSmartFolderLiveRefresh`, or the existing
  open/close-triggered cancel — those are correct; only the destroy-time gap is missing.

## Acceptance criteria
- `onDestroy` cancels the debounce and unlistens the `folder-watch` listener unconditionally, matching
  every other subscription already torn down there.
- A new test proves the leak is closed (mock-based, not a real-timer sleep-and-hope).
- `npx vitest run --pool=forks --poolOptions.forks.singleFork=true` on the full suite completes with no
  new timeouts attributable to this listener/timer (a useful stress check for this specific ticket, not
  a standing requirement for every PR).

**Conflict surface:** `src/App.svelte` (the `onDestroy` block, ~line 6171, and the CPE-1230
live-refresh block, ~line 1986-2031) plus `src/App.smartFolderLiveRefresh.test.ts`. Small, isolated —
does not touch `savedSearchStore.ts` or CPE-1628's conflict surface.
