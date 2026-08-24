---
id: CPE-1884
title: the Drop Stack handle floats permanently over the Sidebar's bottom rows and eats their clicks
type: bug
priority: High
status: Backlog
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

- [ ] A user-level reproduction recorded, with a screenshot, before any fix.
- [ ] The Trash row responds to a click at every supported window size.
- [ ] Every other element the handle can cover is enumerated, and either fixed or explicitly accepted.
- [ ] A test that fails if the handle ever covers an interactive element again.
- [ ] The three `known-failing.json` entries this unblocks are removed (they belong to CPE-1827 and
      CPE-1866 and were deliberately not auto-clearing, because a driver-version fix could never
      resolve them).

## Work Log

- **2026-08-23 20:40 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`.
  Two workers on unrelated tickets converged on it from opposite directions: CPE-1827's chasing why its
  new spec failed on Linux CI, CPE-1866's chasing why three previously-green cases broke under session
  reuse. Neither widened its own PR to fix it, which was the right call on shared files under time
  pressure — and is why it gets its own ticket with the evidence intact.
