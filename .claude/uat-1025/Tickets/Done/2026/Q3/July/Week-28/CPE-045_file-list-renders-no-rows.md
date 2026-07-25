---
id: CPE-045
title: File list renders zero rows while the status bar reports the correct count
type: Bug
status: Done
priority: Critical
component: Frontend
estimate: 2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

v0.5.0 shipped with the file list **completely blank**. Navigating into a folder updated the
breadcrumb, the tab title, the details pane and the status bar ("18 items") — but not a single row
appeared. The app was effectively unusable as a file explorer.

Found by installing the release and looking at it. Every automated check was green.

## Environment

- Cross-Platform Explorer v0.5.0, Windows 11 (WebView2)

## Steps to Reproduce

1. Launch v0.5.0.
2. Click any folder in the sidebar.
3. Column headers render; status bar reports the correct item count; the list area is blank white.

## Expected Behavior

One row per entry.

## Actual Behavior

Zero visible rows, under a correct item count.

## Acceptance Criteria

- [x] Root cause identified
- [x] Rows render in details, list, and icons views
- [x] A regression test exists that FAILS on the unfixed code
- [x] The class of bug (CSS collision) is prevented, not just this instance
- [x] Test infrastructure can actually catch render bugs in future

## Resolution

**Root cause:** `FileList` wrote `class="row {view}"`. With the default view, every row got the bare
class **`details`** — which collides with the *global* `.details` rule that styles the DetailsPane:

```css
.details { display: flex; flex-direction: column; padding: 20px 16px; overflow-y: auto; }
```

That overrode the row's `display: grid` and injected 20px of vertical padding into a 30px-tall row,
clipping every cell to nothing. The rows were in the DOM and correctly populated — they were 18 blank
white strips. The count was right because the *data* was right; only the presentation was destroyed.
A generic class name leaked out of one component and silently ate another.

**Fix:** namespace the view class (`view-details` / `view-list` / `view-icons`) and update the scoped
selectors to match.

**Why nothing caught it.** There were 80 passing tests — all against *pure modules*. Not one test
rendered a component. So this was invisible to the entire suite, to `svelte-check`, and to all four CI
jobs. That is the real defect; the CSS collision was just the symptom.

**What was added, in order of value:**

1. **Component tests** (`FileList.test.ts`) — render the list and assert rows appear.
2. **An integration test** (`App.test.ts`) — render the *real* App against a mocked backend, navigate
   into a folder, and assert rows appear AND that the item count matches the rows actually shown. That
   invariant ("the count must never contradict the list") is precisely what was violated.
3. **A collision guard** — asserts a row never carries a bare class owned by a global layout rule
   (`details`, `content`, `main`, `sidebar`, …). Verified to FAIL on the unfixed code with the exact
   message "row must not carry the reserved global class 'details'". This catches the *class* of bug,
   not just this instance.

Two test-harness bugs had to be fixed to make any of this possible, and both were silently making
integration tests meaningless:
- Vitest resolved Svelte's **SSR** build, so `onMount` never fired — components rendered markup but no
  lifecycle, so a passing test proved nothing. Fixed with `resolve.conditions: ["browser"]` under Vitest.
- `@tauri-apps/*` was pre-bundled, so `vi.mock` couldn't intercept `invoke`. Fixed by inlining it.

Note the honest limit: **jsdom applies no CSS**, so no rendering assertion can catch a CSS collision.
That is exactly why the guard asserts the *cause* (the offending class) rather than the *effect*.

Files: `src/lib/components/FileList.svelte`, `src/lib/components/FileList.test.ts` (new),
`src/App.test.ts` (new), `vite.config.ts`.

## Work Log

2026-07-11 — Installed v0.5.0 and looked at it. List blank, count correct. Confirmed it reproduced across folders, so not a repaint artifact.
2026-07-11 — Checked the built CSS: `.row` had display:grid and a height, so the rules themselves were fine. Ruled out CSS being absent.
2026-07-11 — Wrote a component test for FileList: it PASSED. So the component was fine in isolation — the fault was environmental.
2026-07-11 — Wrote an integration test against the real App. Discovered onMount never fired under Vitest (SSR resolution) and vi.mock couldn't intercept @tauri-apps. Fixed both; the test then passed too — because jsdom applies no CSS.
2026-07-11 — That narrowed it to CSS-only. Re-read the markup: `class="row {view}"` → bare class `details` → collides with the global DetailsPane rule. Found it.
2026-07-11 — Namespaced the view class. Added the collision guard and PROVED it fails on the unfixed code before trusting it.
2026-07-11 — 91 tests green. Closing as Done.

## Notes

The lesson worth keeping: 80 green tests on pure functions gave real confidence and caught real bugs —
but they could not have caught this, because nothing rendered. Coverage of the *logic* is not coverage
of the *product*. The item-count-vs-rows invariant is now asserted, so a blank list under a correct
count can never ship silently again.
