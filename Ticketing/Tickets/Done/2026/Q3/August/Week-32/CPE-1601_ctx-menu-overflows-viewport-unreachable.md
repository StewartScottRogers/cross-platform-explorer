---
id: CPE-1601
title: "ContextMenu.svelte: a tall '.ctx' menu overflows the window with no scroll — rows below the fold are permanently unreachable"
type: Bug
status: Doing
priority: Medium
component: Frontend
epic: CPE-810
tags: [ready]
created: 2026-08-10
closed:
---

## Found while
Triaging `gui-smoke`'s Linux known-failing tail (CPE-1595). `shred-dialog.smoke.ts` right-clicks a
seeded `.txt` file (shreddable, so the menu's separated "Securely delete…" group renders) and asserts
the row is clickable — it timed out with `Error: element (".ctx button.row") still not clickable after
10000ms` on a live CI run (run 31449236678, job 93649941153).

## Evidence (original bug)
- **Screenshot** (`gui-smoke-screenshots-ubuntu` artifact, run 31449236678, `shred-dialog-fail.png`):
  the menu is open and fully populated from "Open" down through "Metadata Studio…", which sits right at
  the bottom edge of the window. "Securely delete…" (which the spec targets) and the trailing
  "Documents for this view" row are not visible at all — pushed past the bottom of the app window.
- **Job log** (same run): WebdriverIO's own clickability polyfill (`isElementClickable`) repeatedly
  computed `isElementInViewport(elem) === false` for the target button, and the browser-side
  `getComputedStyle` call ruled out `display:none` (`{ value: 'flex' }`) — the row is genuinely
  rendered, just positioned outside `window.innerHeight`.
- **Code** (`src/lib/components/ContextMenu.svelte`): the `onMount` viewport clamp only ever
  *repositioned* the menu's top-left corner — it never accounted for the menu's own height exceeding
  the available window height. `.ctx` had no `overflow`/`max-height` set at all, so a tall menu simply
  overflowed the window edge with nothing to scroll.
- **Why this is more than "off-screen at that instant"**: WebdriverIO's own clickability check (and a
  real user's mouse-wheel/keyboard nav) works by calling `elem.scrollIntoView()` on the target — a
  no-op with no scroll container to act on, so the row was **permanently** unreachable. Also a direct
  violation of docs/design/MENUS.md's own container rule: "clamped into the viewport (never clipped
  off-screen)".

## Round 1 fix — and the regression it introduced (caught in review, not shipped)
`.ctx` got `max-height: calc(100vh - 12px); overflow-y: auto;`, making it a scroll container so
`scrollIntoView()` has something to act on. **This part is correct and stands.** But it shipped in the
same PR without accounting for `Submenu.svelte`'s `.flyout` — the nested panel behind New ▸ / View ▸ /
Sort by ▸ / Run macro ▸ / Run command ▸, which is an absolutely-positioned child of `.submenu` (itself a
descendant of `.ctx`) that escapes horizontally via `left: 100%`.

**Two independent reviewers (the coordinator's own UAT pass and a separate independent code reviewer)
each caught this before merge, with their own real-browser repros:**
- Per the CSS Overflow spec, a `visible` axis computes to `auto` the moment the OTHER axis is anything
  but `visible` — so setting only `overflow-y: auto` makes `overflow-x` compute to `auto` too.
  `overflow-x: visible` does **not** opt back out (verified directly by both reviewers:
  `getComputedStyle(.ctx).overflowX` still returns `"auto"`).
- That makes `.ctx` a clipping container on **both** axes, and `.flyout` — an absolutely-positioned
  descendant deliberately escaping `.submenu`'s box — gets clipped to nothing. Opening any flyout
  rendered **fully invisible**, and even a short, single-row menu with no vertical overflow at all
  sprouted spurious scrollbars the instant a flyout opened (the flyout's rightward escape contributes
  to `.ctx`'s scrollable-overflow region even though it's clipped from view).
- This is **more common** than the original bug: `New ▸` is used constantly, on every item and
  empty-area menu, not just rich/tall ones.

## Round 2 fix — flyout escapes via `position: fixed`, not CSS overflow tricks
`overflow-x: hidden` does not work either — the flyout still escapes horizontally, so hiding that axis
clips it exactly the same way. The real fix is for the flyout to stop being a clippable descendant of
`.ctx` at all.

`src/lib/components/Submenu.svelte`: `.flyout` is now `position: fixed` (was `position: absolute` with
CSS `top:-6px; left:100%`), with `top`/`left` computed in JS (`positionFlyout()`) from the parent row's
`getBoundingClientRect()`. A `position: fixed` box is positioned against the viewport (the initial
containing block) and — critically — is **not** part of any ancestor's scrollable-overflow computation,
so `.ctx`'s scroll/clip no longer touches it at all. (This only holds because no ancestor between here
and the viewport sets `transform`/`filter`/`perspective`/`will-change:transform`/`contain:paint`, any of
which would make *that* ancestor the fixed containing block instead — none of `.ctx`/`.submenu`'s
ancestors do.)

Preserved/added along with the positioning change:
- The existing right-edge flip clamp, re-derived against the viewport (previously measured
  `.flyout`'s own rect against `window.innerWidth`; now the same math, just also computing `top` and
  clamping it vertically too — the old CSS never handled a flyout taller than the viewport).
- A `scroll` listener on `window` in the capture phase (catches scroll on **any** ancestor, including
  `.ctx` itself, even though `scroll` events don't reliably bubble across every engine this app targets)
  that re-anchors the flyout to its parent row's current rect — so it tracks correctly if the (now
  scrollable) `.ctx` is scrolled while a flyout is open, instead of drifting or freezing in place.
- Outside-click/Escape dismissal is unchanged (still handled by `ContextMenu.svelte`'s window-level
  listeners plus `Submenu.svelte`'s own mouseleave/Escape handling) — none of that depended on the
  flyout's CSS positioning scheme.

## Verification (this round)
**jsdom cannot do layout**, so a green vitest suite proves nothing about clipping — the whole 51-test
suite was green while the round-1 regression was live. Verified instead:

1. **Structural regression guard (jsdom, `ContextMenu.test.ts`)**: a new test opens the "New ▸" submenu
   and asserts `flyoutEl.style.position === "fixed"` — pins the mechanism itself so a future edit can't
   silently revert to a clippable `position: absolute` without failing a test. (Can't verify the
   clipping consequence in jsdom — see next point — only that the escape mechanism is still in place.)
2. **Real-browser repro**: rebuilt `.ctx`/`.submenu`/`.flyout`'s exact CSS and DOM nesting from both
   components (post-fix) in a static page, loaded in real Chrome via the `claude-in-chrome` tools, and
   drove it with `getBoundingClientRect()`/`getComputedStyle()`/screenshots across four scenarios:
   - **Short menu**: flyout opens with real nonzero size (190×76px), fully within the viewport,
     `position: fixed` confirmed via computed style; `.ctx`'s `scrollWidth`/`scrollHeight` stayed equal
     to `clientWidth`/`clientHeight` with the flyout open — **no spurious scrollbars**.
   - **Tall, scrolled menu**: flyout (198×44px) fully visible before scrolling; after scrolling `.ctx`
     by 80px, the flyout's `top` moved by exactly 80px — confirmed it **tracks its parent row** rather
     than freezing or drifting.
   - **Right-edge menu**: flyout flipped leftward (`data-flip="true"`) and stayed fully within the
     viewport (`left=885, right=1075` inside a 1314px-wide viewport).
   - **Negative control**: rebuilt the ORIGINAL (`position: absolute; left: 100%`) flyout inside a fresh
     `overflow-y:auto` container as a sanity check on the repro methodology itself — confirmed it
     reproduces the exact reported failure: `getComputedStyle().overflowX` resolved to `"auto"` with no
     `overflow-x` set, the container showed both a horizontal and vertical scrollbar even for a
     two-row menu, and `document.elementFromPoint()` at the flyout's own first-button coordinates
     returned a DIFFERENT element (the clipped flyout is not the topmost paintable element there) —
     screenshotted and visually confirmed the flyout renders **fully invisible**, exactly matching both
     reviewers' independent findings.
3. **`ContextMenu.test.ts`**: 52/52 passing (51 pre-existing + 1 new). **`gui-smoke`**: `npm run
   typecheck` clean, `npm run test:unit` 32/32 passing.
4. **`gui-smoke/specs/macro-in-menu.smoke.ts`** (CPE-1191's existing "Run macro ▸" spec — the exact
   real-app scenario the reviewers flagged): strengthened its assertion. It previously only checked
   `waitForExist` + `getHTML()` on `.ctx .flyout`, which structurally cannot detect CSS clipping (the
   element still exists in the DOM with the right markup even when fully clipped — this is *why*
   nothing caught the round-1 regression before review). Now also asserts the flyout's
   `getBoundingClientRect()` is nonzero-sized and fully within `window.innerWidth`/`innerHeight` — the
   same "did it actually paint, not just mount" check this ticket's own real-browser repro relies on.
   Not verified on live CI (this session cannot run `tauri-driver`/WebKitGTK — see CPE-1595's Notes).

## Round 3 — orphaned flyout on scroll, unclamped left flip, and CPE-1601's own bug one level deeper
PR #808 round 2's code-reviewer pass approved; UAT failed it on a third, narrower issue both agents found
independently with their own real-browser repros, which the coordinator adjudicated as blocking. Two
more were folded in from the same review pass (filed as CPE-1610; now resolved here — see below).

1. **Orphaned flyout (the blocking one).** Round 2's `window` scroll listener only ever
   *repositioned* the `position:fixed` flyout — it never asked whether the anchor row was still visible
   at all. Scroll `.ctx` (dragging the scrollbar thumb, or a fast trackpad/momentum scroll — neither
   triggers `mouseleave`, since the cursor never leaves the row or the scrollbar track) far enough that
   the anchor row's rect goes fully outside `.ctx`'s box (e.g. `top:-87, bottom:-55`), and the flyout
   stayed open, clamped to `top:6px`, floating adjacent to nothing and overlapping unrelated rows — a
   popup detached from the thing that opened it. Fix: `Submenu.svelte`'s `onAncestorScroll` now walks up
   from the parent row to find the nearest actual scroll/clip ancestor (`nearestScrollAncestor` — found
   generically via computed `overflow-y`, not hard-coded to `.ctx`, so this keeps working if `Submenu` is
   ever reused elsewhere) and, if the anchor row's rect falls fully outside that ancestor's box
   (`anchorScrolledOut`: `pr.bottom < cr.top || pr.top > cr.bottom`), **closes** the submenu instead of
   repositioning it.
2. **Left flip had no bounds check.** `positionFlyout()` clamped `top` but computed `left` as a bare
   binary flip with no floor/ceiling — reproduced live at `left: -184px` (off-screen, nothing pulling it
   back) using a `.ctx` wide enough and close enough to the left edge to trigger the flip while its own
   left edge sat near `x:0`. Fixed with the same `Math.max(pad, Math.min(...))` clamp `top` already used.
3. **A flyout taller than the viewport had no scrollbar — CPE-1601's own bug, one level deeper.**
   `.flyout` had no `max-height`/`overflow-y`; `macros`/`userCommands` are unbounded lists, and a long
   one (tested: 40 rows) rendered past the bottom of the window with nothing to scroll it into view.
   Fixed with `max-height: calc(100vh - 12px); overflow-y: auto;` — the exact fix `.ctx` itself got in
   round 1, deliberately re-applied here with a comment naming the round-1 overflow-x-becomes-auto trap
   explicitly, and noting it's safe ONLY because `.flyout` has no nested submenu of its own today; a
   future nested flyout must repeat the `position:fixed`-anchored-to-parent-rect treatment, not lean on
   CSS overflow/positioning, or it will hit the exact bug round 2 did.

**Verification (round 3):** real Chrome again (jsdom can't detect any of these three — geometry-blind).
Rebuilt the repro harness with the fixed `positionFlyout()`/`onAncestorScroll()` logic ported verbatim:
- **Orphan fix + negative control**: scrolled a tall `.ctx` far enough that the anchor row's rect (`top:
  256, bottom: 288`) fell fully outside the `.ctx` box (`top: 400, bottom: 550`). With the fix: flyout
  closed (`isOpen() === false`, removed from DOM). With the OLD (round-2) reposition-only logic run
  side-by-side in the same page: the flyout stayed open, repositioned to `top: 250`, screenshotted
  floating in empty space with no connection to any visible row — the exact reported defect, reproduced
  on demand, then shown resolved.
- **Left clamp**: a wide `.ctx` positioned near the left edge, sized to also trigger the flip, produced
  a pre-clamp `left: -184` (would-be off-screen); the shipped clamp brought it to `left: 6` (`right: 196`,
  fully inside a 984px-wide viewport).
- **Tall flyout**: a 40-row flyout rendered at exactly `height: 509px` in a 521px-tall viewport (`100vh -
  12px` = `521 - 12 = 509`, an exact match), `bottom: 515 <= 521` (fits), `scrollHeight: 1290 >
  clientHeight: 507` (internally scrollable — every row still reachable).
- Also reconfirmed round 1/2 still hold (short menu no spurious scrollbars, tall-menu tracking, right-edge
  flip) — none of that regressed.

**Coverage added:** two jsdom tests in `ContextMenu.test.ts` (stubbing `getBoundingClientRect` directly,
no real layout needed) pinning the orphan-close logic itself — one asserting the flyout closes once the
anchor's rect is fully outside `.ctx`'s, one asserting a still-(partially)-visible anchor does NOT close
its flyout (guards against an over-eager close). `ContextMenu.test.ts` now 54/54. Did not extend
`gui-smoke/specs/macro-in-menu.smoke.ts` further for the orphan case specifically — that spec's fixture
(a single right-click, not inside a pre-populated tall/scrolled menu) doesn't naturally reach a
scroll-out state without a contrived setup; flagged as a possible future addition rather than forced in
here.

**CPE-1610** (filed by the sprint process from this same review round, covering items 2 and 3 above plus
this ticket's own orphan item under a shared "same file, same class of bug" umbrella) is resolved by this
round and deleted from `Ticketing/Tickets/Backlog/` in the same commit — nothing in it remains open.

## Still open
Live CI confirmation, across all three rounds. `gui-smoke/known-failing.json`'s `shred-dialog.smoke.ts`
entry stays listed until a real Linux CI run confirms the spec passes.

## Follow-up (not done here, out of this ticket's evidence base)
`AgentMenu.svelte` and `TabMenu.svelte` also define their own local `.ctx` with no `max-height`/
`overflow-y` — likely share the ORIGINAL (round-1) defect, but neither was exercised by a failing
gui-smoke spec, so unproven live. Worth an audit + the same fix pattern (both the `.ctx` scroll
container AND checking whether either has its own flyout/nested-panel escaping trick that would need
the same `position: fixed` treatment) next time either is touched.
