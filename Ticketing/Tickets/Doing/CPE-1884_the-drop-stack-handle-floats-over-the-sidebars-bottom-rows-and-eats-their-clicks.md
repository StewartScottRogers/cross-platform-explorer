---
id: CPE-1884
title: the Drop Stack handle floats permanently over the Sidebar's bottom rows and eats their clicks
type: bug
priority: High
status: Doing
tags: ready
estimate: M
created: 2026-08-23
closed:
---

## Problem

`DropStackPanel.svelte`'s `.drop-stack-handle` toggle is `position: fixed; left: 14px; bottom: 14px;
z-index: 149` and is rendered **unconditionally** — only the *expanded* panel is behind `{#if open}`,
the handle itself always floats. `Sidebar.svelte`'s Trash section sits at `order: 900`, near the
bottom of a fully-expanded sidebar. At ordinary window heights the two occupy the same pixels, and the
handle wins.

**Measured**, not inferred, by the CPE-1827 worker from a real CI job log and its screenshot artifact
(run `32676997154`, shard 4, job `97288403795`):

```
WebDriverError: element click intercepted   (clicking the Sidebar's "Open Trash" row)
  Open-Trash row rect at click time : { x: 20, y: 644, width: 193, height: 30 }   (1000×700 window)
  .drop-stack-handle                : x ≈ 14–125, y ≈ 658–686
```

The rects genuinely overlap. Confirmed independently by the CPE-1866 worker while chasing an unrelated
failure: WebdriverIO's own built-in click-intercepted recovery — scroll-into-view, pointer move, retry
— **also fails on the same node**, twice, and the interception blocks both a plain `.click()` and a
CDP/Actions `rightClick`. That rules out a client-library quirk and points at a real DOM-level overlay.

## Why High — this is a user-facing bug, not a test problem

It surfaced through tests, but nothing about it is test-specific. A real user with an expanded sidebar
at a modest window height clicks the Trash row and **nothing happens**, because their click lands on
an invisible-to-them floating handle instead. There is no error, no feedback, no clue — the row simply
does not respond.

That is the worst class of UI bug to report: the user experiences "the app felt weird for a second"
and has nothing to describe. It has plausibly been happening for a long time and would never arrive as
a coherent bug report.

Note the aggravating factor: the handle is `position: fixed`, so it does not scroll away with the
sidebar's content. Making the sidebar taller or scrolling it does not help.

## What to do

1. **Reproduce it as a user first**, at a window height where the two overlap, and record what a click
   on the Trash row does. Screenshot it. That is the ticket's real evidence; the CI logs are the trail
   that found it.
2. Decide the fix deliberately, because there are several and they are not equivalent:
   - give the handle `pointer-events: none` when collapsed and put them on its inner hit target only
     (smallest change, but check the collapsed handle is still clickable);
   - reserve space for it so sidebar content cannot flow underneath;
   - move or hide the handle when the drop stack is empty — worth asking whether an always-visible
     floating control is right at all when there is nothing in the stack;
   - raise the sidebar's own stacking so its rows win.
3. **Check what else is down there.** The Trash row is what the tests happened to click. Any sidebar
   section at a high `order` is exposed, and so is anything else the app draws in that corner. Enumerate
   the victims rather than fixing the one that was caught.
4. **Add a guard.** This is precisely the class **CPE-1882** exists for — a real-browser layout check
   answering "does element A paint over element B". If CPE-1882 has landed, add the assertion there. If
   not, note in that ticket that this is a concrete case it must catch.

## Acceptance criteria

- [x] A user-level reproduction recorded, with a screenshot, before any fix.
- [x] The Trash row responds to a click at every supported window size.
- [x] Every other element the handle can cover is enumerated, and either fixed or explicitly accepted.
- [x] A test that fails if the handle ever covers an interactive element again.
- [ ] The three `known-failing.json` entries this unblocks are removed (they belong to CPE-1827 and
      CPE-1866 and were deliberately not auto-clearing, because a driver-version fix could never
      resolve them). **Left unchecked/entries left in place** — see Work Log: 4 entries (not 3) match,
      all tagged CPE-1822, and I could not execute the actual WebdriverIO/WebKitGTK CI job that
      produced them (msedgedriver is version-mismatched and hangs here per the Foreman's steer, and
      there is no Linux/WebKitGTK available locally), so I have not *proven* they pass and won't remove
      an entry I haven't proven.

## Work Log

- **2026-08-23 20:40 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`.
  Two workers on unrelated tickets converged on it from opposite directions: CPE-1827's chasing why its
  new spec failed on Linux CI, CPE-1866's chasing why three previously-green cases broke under session
  reuse. Neither widened its own PR to fix it, which was the right call on shared files under time
  pressure — and is why it gets its own ticket with the evidence intact.

- **2026-08-23 (Worker)** — Picked up. Reproduced as a user first: drove plain installed
  `chrome.exe --headless=new` via raw CDP against the REAL app (the project's own `vite` dev server,
  not a stand-in) at 1000×570, screenshotted it (the "Drop Stack" pill visibly sitting on top of the
  Trash section), then proved the click failure directly — dispatched a real mouse click at "Reset
  section order"'s own on-screen point and confirmed via `localStorage['cpe.sidebarOrder']` that the
  handler never fired. Same result for the Trash child row.

  **Root cause, precisely**: `.navigation-pane` (Sidebar.svelte's scroll container) gets
  `flex: 1 1 auto` from its `.pane-col` parent (`overflow: hidden`, fixed height) — so its own
  rendered box, not its content, determines how far down it paints. At ordinary window heights that
  box already reaches within `padding-bottom: 12px` of the window's bottom edge, well inside
  DropStackPanel's always-mounted `.drop-stack-handle` (`position: fixed; bottom: 14px`, ~28px tall,
  `z-index: 149`), so the pane's own bottom rows physically render underneath it.

  **Victims enumerated** (not just Trash): the Sidebar's Trash header, its "Open Trash" (or, on a
  platform where `canBrowseTrash` is false, the disabled "Open Finder's Trash instead" message) child
  row, and the always-last "Reset section order" row — all three sit at fixed `order: 900`/`1000`,
  past every drag-reorderable section, so they are *structurally* always the pane's last content
  regardless of section order. Also implicated by the same mechanism, though not independently
  confirmed by name: whichever reorderable section (Agents/Favorites/Tags/Smart/Saved
  Search/Explore/Places/Drives/Network) a user drags to the very end — same failure mode, since it's
  the pane's *box*, not any specific row, that was reaching the handle.

  **Fix chosen (of the ticket's four options): reserve space.** `src/app.css` — `.navigation-pane`
  gets `margin-bottom: 50px` (matching `.drop-stack-panel`'s own existing `bottom: 50px`, i.e. the
  same "clear the handle" offset already established elsewhere in this file). Tried inner
  `padding-bottom` first and it does NOT work: `.navigation-pane`'s box height comes from its flex
  parent, not its content/padding, so padding only grows the *scrollable* area past the last row
  without moving anything already on screen. A bottom MARGIN, being outside the scroll box, genuinely
  shrinks the box itself, so its visible/scrollable bottom edge sits 50px above the window bottom at
  every height — nothing inside it can ever paint into the handle's y-range again, which is a complete
  fix for the whole class (any current or future last-row), not a patch for the Trash instance.
  Rejected `pointer-events: none` (smallest diff, but only helps clicks, not the visual overlap, and
  is easy to get subtly wrong on the collapsed handle's own hit target) and "raise the sidebar's own
  z-index" (the sidebar's rows are normal-flow, not positioned — winning the stacking order would just
  paint the pane's opaque background over the handle wherever they'd have overlapped, an ugly visible
  seam, not a real fix). Left the handle visible-when-stack-is-empty as is — out of scope for a
  click-interception bug, and the reserved-space fix makes that question moot for correctness either way.

  **Verified**: `npm run check` clean; `npm run test` — 331 files / 4454 tests green. Confirmed the
  collapsed Drop Stack handle button is still clickable after the fix (aria-expanded flips, panel
  opens). Swept 14 window heights (420–1000px) confirming the handle's own painted footprint is always
  exactly itself, never a sidebar element.

  **Guard added**: `scripts/dev-harness/sidebar-drop-stack-overlap/check.mjs` (+ `npm run
  harness:sidebar-drop-stack-overlap`) — spins up the project's own `vite` dev server, drives headless
  Chrome via raw CDP across 11 window heights, and asserts the structural invariant the fix provides
  (`.navigation-pane`'s own box never extends into the handle's y-range) plus a row-level paint-probe
  at the natural load position. Red-proofed twice by deliberately reverting the CSS: v1 of the probe
  (checking the handle's own corners) never failed even with the bug reintroduced — elementFromPoint at
  the handle's own rect trivially always returns the handle, since z-index means it wins there by
  definition; v2 (checking every row's own click-center) produced false positives from rows simply
  scrolled out of the pane's own clip, unrelated to this bug. The final version — assert the pane's box
  stays above the handle, plus a clip-aware row probe — fails at all 11 heights with the bug back, and
  passes at all 11 with the fix. **Not wired into CI**: that's CPE-1882's explicit scope (generalising
  + CI-wiring this exact class of real-browser layout check); left a note on CPE-1882 naming this as a
  third concrete case for it to pick up, alongside the two it already lists.

  **`known-failing.json`**: 4 entries in `gui-smoke/known-failing.json` (not 3 — the ticket text says
  three, the "What matters" section says "CPE-1827's four", the file has four, all tagged `CPE-1822`)
  cite exactly this bug (`trash-titlebar.smoke.ts`'s four cases, root-caused via CI run
  `32676997154`/job `97288403795` to this same handle/Trash-row overlap at 1000×700). I did not remove
  them. I'm confident the root cause is gone (the structural proof above is font/engine-independent —
  it's a pure CSS box-model guarantee, not dependent on WebKitGTK's specific text metrics), but those
  entries were produced by a real WebdriverIO run against `ubuntu-latest`/WebKitGTK in CI, and per this
  ticket's own instruction I won't remove an entry I haven't proven passes. I could not run that job:
  msedgedriver here is version-mismatched against the installed Edge and hangs (the Foreman's explicit
  steer for this ticket), and there is no Linux/WebKitGTK available locally to substitute. Left for CI
  (or the Foreman) to confirm-and-clear once `trash-titlebar.smoke.ts` runs green on this fix.

  **Assumption**: `.navigation-pane`'s 50px margin is a static reserve (not conditional on actual
  window height), matching the codebase's existing `.drop-stack-panel { bottom: 50px }` convention —
  deliberate per the ticket's own "reserve space" option, trading a small amount of always-present
  blank space at the bottom of a very tall sidebar for a fix that needs no resize-observer and cannot
  drift out of sync with the handle's real footprint.
