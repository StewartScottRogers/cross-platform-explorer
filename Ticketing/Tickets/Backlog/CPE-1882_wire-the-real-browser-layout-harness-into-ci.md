---
id: CPE-1882
title: wire the real-browser layout harness into CI, so a clipping regression goes red instead of needing a human
type: task
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-23
closed:
---

## Why this is the highest-leverage QA ticket open

For this entire batched run the Visual Critic has been **blind**: nothing local could answer a
*layout* question, so every visual ticket shipped on jsdom assertions plus a human's judgement.

The cause was misdiagnosed twice. First as "the GUI test drivers are not installed" — they are, in
`~/.cargo/bin`. Then as "`msedgedriver` 150 against Edge 151 hangs sessions" — true, and still worth
fixing, but **not the blocker**.

The actual answer was found by the worker on CPE-1833/CPE-1836 while doing something else:
**plain installed `chrome.exe --headless=new`**, driving a local page. No WebDriver. No install. No
machine-global change. Living proof in `scripts/dev-harness/statusbar-notice/`, which now reports:

- element **rects** at a chosen viewport width
- an **`overlapPairs`** list — which elements actually collide
- a **paint probe** (`elementFromPoint`) answering "does element A paint on top of element B"

That is the missing capability, and it is not screenshots. jsdom can assert that a CSS property
appears in the source; it can **never** tell you whether the resulting pixels overlap. That is exactly
why "the git block bleeds into the disk label" (CPE-1836) was a real, visible bug that no test could
catch.

## The gap this ticket closes

**None of that thoroughness reaches CI.** PR #1019's reviewer grepped for it: the harness is invoked
by nothing in `gui-smoke` or `vitest`. The only CI guard for CPE-1836 is three regex assertions
against the `<style>` source text. That reviewer enumerated precisely what those miss:

- a second `.git{}` rule added later in the file (or with `!important`) overriding the first — the
  helper uses `.match()`, not `.matchAll()`, so it only inspects the first occurrence
- any layout regression not touching those four specific property/selector pairs, e.g. a new pinned
  child added to `.git` without `flex: 0 0 auto`

So the current guard is a **narrow tripwire for one fix**, not a layout guarantee — and the run's own
experience says the next clipping bug will be in a different component anyway.

## What to do

1. **Generalise the harness.** Something a ticket can point at a component plus a list of widths and
   get back rects, overlap pairs and paint probes. `scripts/dev-harness/statusbar-notice/` is the
   working prototype — read it first; do not start over.
2. **Wire it into CI** as its own job. It needs no WebDriver, so it should be far more reliable than
   the existing `gui-smoke` legs, which is most of the point.
3. **Red-proof it** against the two bugs already on record: reintroduce CPE-1836's missing
   `overflow: hidden` and confirm the job goes red naming the overlapping pair; do the same for
   CPE-1827's Trash titlebar at the 600px floor.
4. **Cover the standing rule, not just the two bugs.** The repo's pill/tick-tack convention says a row
   of pills must wrap and grow while each pill keeps its text on one line. That rule has no automated
   enforcement anywhere. A generic "no element in this row overlaps another, and no text overflows its
   own background" check would enforce it everywhere at once.

## Fixes a mis-referenced acceptance criterion

CPE-1836's own AC says its fix must be *"pinned by the browser-level coverage from CPE-1822"*.
CPE-1822 is entirely about `gui-smoke` coverage for the **Trash view** and has nothing to do with the
status bar — the reference is wrong and could not have been satisfied by anyone touching
`StatusBar.svelte`. Found by PR #1019's reviewer, which read CPE-1822 rather than assuming. **This
ticket is the correctly-scoped replacement for that bullet.**

## Relationship to the driver mismatch

Fixing `msedgedriver` against the installed Edge (recorded in
`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`) is still worth doing for **full-app flows** — real
navigation, real Tauri commands, real trash operations. But it is no longer on the critical path for
**layout** claims, and this route is cheaper, faster and has fewer moving parts. Do this first.

## Acceptance criteria

- [ ] A CI job that measures real layout for at least two components at multiple widths.
- [ ] Reintroducing CPE-1836's bug makes it red, naming the overlap — demonstrated.
- [ ] Reintroducing CPE-1827's bug makes it red — demonstrated.
- [ ] A ticket author can add a component and a width list without touching harness internals.
- [ ] `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` updated: this closes the layout half of the
      GUI-verification debt, and the row says so.

## Work Log

- **2026-08-23 18:45 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`.
  Three separate agents converged on this today from different directions: the CPE-1833/1836 worker
  built the harness, PR #1019's reviewer proved it never reaches CI and found the mis-referenced AC,
  and the CPE-1827 worker independently lost hours to the driver mismatch this route sidesteps.
