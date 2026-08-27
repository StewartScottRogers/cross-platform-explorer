---
id: CPE-1882
title: wire the real-browser layout harness into CI, so a clipping regression goes red instead of needing a human
type: task
priority: High
status: In Progress
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

- **2026-08-23 (CPE-1884 worker)** — A third concrete red-proof case for item 3 (alongside CPE-1836's
  status bar and CPE-1827's Trash titlebar): CPE-1884 (the Drop Stack handle floating over the
  Sidebar's bottom rows) is the same class of bug — `.drop-stack-handle` (`position: fixed`) painting
  over Sidebar.svelte content — and I built a standalone version of exactly this harness while fixing
  it, since this ticket hadn't landed yet: `scripts/dev-harness/sidebar-drop-stack-overlap/check.mjs`
  (`npm run harness:sidebar-drop-stack-overlap`). Same approach this ticket already specifies — plain
  `chrome.exe --headless=new` + raw CDP (`Runtime.evaluate`/`Emulation.setDeviceMetricsOverride`), no
  WebDriver, no npm deps — but spins up its own `vite` dev server and drives the REAL app rather than a
  purpose-built stand-in page, and asserts a structural containment invariant
  (`.navigation-pane`'s own rendered box never extends into `.drop-stack-handle`'s y-range) rather than
  per-pixel overlap pairs. Red-proofed twice by deliberately reverting the CPE-1884 fix — worth reading
  before generalising: v1 of my probe (checking the handle's own corners) never failed, because
  `elementFromPoint` at an element's own rect trivially returns that element (it wins the paint order
  there by definition — checking the WRONG side's corners can never observe this class of bug); v2
  (checking every row's own click-center) produced false positives for rows simply scrolled outside
  the container's own clip, unrelated to the actual defect. Not wired into CI — left for this ticket.
  See CPE-1884's Work Log for the full writeup (repro screenshots, before/after evidence, the fix
  itself) and `gui-smoke/known-failing.json`'s four `trash-titlebar.smoke.ts` entries (tag `CPE-1822`)
  it could not itself clear (same msedgedriver/WebKitGTK gap this ticket exists to route around).

- **2026-08-26 (Worker)** — Picked up. Plan: generalise `statusbar-notice`'s prototype into
  `scripts/dev-harness/layout-guard/` (a CDP-driving engine with four composable check kinds —
  `siblingOverlap`, `clipProbe`, `textOverflow`, `selfPaint` — reusing `sidebar-drop-stack-overlap`'s
  CDP-over-`chrome.exe --headless=new` shape rather than the outer/iframe `--dump-dom` shape, since
  `Emulation.setDeviceMetricsOverride` sets the real CSS viewport directly and needs no iframe trick), a
  `cases.mjs` manifest (the one file a future ticket touches to add a case), and ONE shared dev server
  (`vite.harness.layout-guard.config.ts`) with a generic, pluggable backend mock
  (`shared-mocks/invoke.ts`'s `registerRawInvoke`) so a new case never needs a bespoke mock file either.
  Two cases: `statusbar-notice` (reuses the existing harness page, red-proofs CPE-1836) and a new
  `trash-titlebar` (new harness page mounting the real `TrashView.svelte`, red-proofs CPE-1827). Wiring
  into `.github/workflows/gui-smoke.yml` as a new job, unconditional on every push/PR (measured cost:
  under a minute end to end — cheap enough that path-filtering isn't worth the CPE-1893-shaped risk of a
  silently-skipped check).

  First local run against the real (fixed) code caught the engine's own bugs, not real regressions —
  worth recording since they shaped the final design: (1) `scrollWidth > clientWidth` alone is NOT "text
  overflows its own background" — `.git-branch`/`.disk`/etc. correctly ellipsis-truncate
  (`overflow: hidden; text-overflow: ellipsis`), which legitimately makes scrollWidth exceed
  clientWidth while painting nothing outside the box; the `textOverflow` check now also requires
  `getComputedStyle(el).overflowX === "visible"` before flagging. (2) `.resize-grip`
  (`position: absolute; right: 0; bottom: 0`) is BY DESIGN allowed to sit over the tail of trailing flow
  content in the corner — added as `siblingOverlap`'s `exclude` option, matching the original CPE-1836
  prototype's own judgment call. Both false positives are recorded in engine.mjs's own comments so a
  future case doesn't rediscover them.

  **Red-proofed both AC-mandated bugs, locally, against the real components:**
  - CPE-1836: removed `.git { overflow: hidden }` in `StatusBar.svelte` → `layout-guard` went red at
    600px: `CLIP-BREACH .git: .git .git-btn:not(.resolve) overhangs by 16.1px AND paints there (probe
    (547.2,13.5) hit .git-btn) — not clipped`. Restored the line → clean at all 12 case/width
    combinations again (confirmed `git diff` shows zero change to `StatusBar.svelte`).
  - CPE-1827: reintroduced the pre-fix shape in `TrashView.svelte` (dropped `.tv-title`'s
    `flex-wrap: wrap` back to the old pinned width, and added 5 dummy buttons to `.tv-tools` to restore
    the old toolbar density) → `layout-guard` went red at the app's own 600px/640px floor: `TEXT-OVERFLOW
    .tv-title scrollWidth=91 clientWidth=0 overflow-x=visible — text paints past its own background`.
    Reverted both → clean again (confirmed `git diff` shows zero change to `TrashView.svelte`).

  Both real components ship unchanged — only the harness itself (engine.mjs/cases.mjs) is a permanent
  diff. Next: wire the `layout-guard` job into `.github/workflows/gui-smoke.yml`, run `npm run
  check`/`npx vitest run`, update `MANUAL-TEST-BURNDOWN.md`, open the PR.
