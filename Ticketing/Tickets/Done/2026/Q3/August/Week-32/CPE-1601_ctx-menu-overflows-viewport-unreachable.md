---
id: CPE-1601
title: "ContextMenu.svelte: a tall '.ctx' menu overflows the window with no scroll — rows below the fold are permanently unreachable"
type: Bug
status: Done
priority: Medium
component: Frontend
epic: CPE-810
tags: [ready]
created: 2026-08-10
closed: 2026-08-10
---

## Found while
Triaging `gui-smoke`'s Linux known-failing tail (CPE-1595). `shred-dialog.smoke.ts` right-clicks a
seeded `.txt` file (shreddable, so the menu's separated "Securely delete…" group renders) and asserts
the row is clickable — it timed out with `Error: element (".ctx button.row") still not clickable after
10000ms` on a live CI run (run 31449236678, job 93649941153).

## Evidence
- **Screenshot** (`gui-smoke-screenshots-ubuntu` artifact, run 31449236678, `shred-dialog-fail.png`):
  the menu is open and fully populated from "Open" down through "Metadata Studio…", which sits right at
  the bottom edge of the window. "Securely delete…" (which the spec targets) and the trailing
  "Documents for this view" row are not visible at all — pushed past the bottom of the app window.
- **Job log** (same run): WebdriverIO's own clickability polyfill (`isElementClickable`) repeatedly
  computed `isElementInViewport(elem) === false` for the target button, and the browser-side
  `getComputedStyle` call ruled out `display:none` (`{ value: 'flex' }`) — the row is genuinely
  rendered, just positioned outside `window.innerHeight`.
- **Code** (`src/lib/components/ContextMenu.svelte`): the `onMount` viewport clamp
  (`left = Math.min(x, window.innerWidth - rect.width - pad); top = Math.min(y, window.innerHeight -
  rect.height - pad); ... = Math.max(pad, ...)`) only ever *repositions* the menu's top-left corner —
  it never accounts for the menu's own `rect.height` exceeding the available window height. When it
  does (a rich "item" menu can render 15-20+ rows: quickrow, Open, New▸, Duplicate, Copy as
  path/to-folder/name, Add to Drop Stack, the compress family, Tags, macros/user-commands, Reveal,
  Properties, Metadata Studio, the separated Securely-delete group, Documents for this view), the
  `Math.max(pad, ...)` clamp re-floors `top` back to `pad`, and the menu's bottom simply overflows past
  the window edge with **no scroll container** — `.ctx` had no `overflow`/`max-height` set at all.
- **Why this is more than "off-screen at that instant"**: WebdriverIO's own clickability check (and a
  real user's mouse-wheel/keyboard nav) works by calling `elem.scrollIntoView()` on the target. That
  only does anything if some ancestor is an actual scroll container. `.ctx` wasn't one — the browsing
  context (this app's fixed-chrome window) has nothing above it to scroll either — so `scrollIntoView()`
  was a genuine no-op and the row was **permanently** unreachable, not just unlucky timing. This also
  directly violates docs/design/MENUS.md's own stated container rule: "placed at the cursor and
  **clamped into the viewport (never clipped off-screen)**" — the clamp existed but only handled
  position, not overflow.

## Fix
`src/lib/components/ContextMenu.svelte`: gave `.ctx` `max-height: calc(100vh - 12px); overflow-y:
auto;`. This makes the menu its own scroll container once its content is taller than the viewport, so
`scrollIntoView()` (WebdriverIO's or a real user's) has something to act on and every row becomes
reachable regardless of how many conditional rows a given selection enables. `onMount`'s existing
position clamp is untouched (and now works correctly by construction: `rect.height` is capped by the
CSS before the clamp reads it, so `window.innerHeight - rect.height - pad` can no longer go negative).

Landed in CPE-1595's PR (branch `cpe-1595-gui-smoke-triage`) alongside the `gui-smoke` triage that found
it, per that ticket's "Do NOT touch src/… unless fixing a genuine product bug you have proven with
evidence" rule.

## Follow-up (not done here, out of this ticket's evidence base)
`AgentMenu.svelte` and `TabMenu.svelte` also define their own local `.ctx` with no `max-height`/
`overflow-y` — MENUS.md already lists them as following the same `.ctx` + `.row` pattern, so they likely
share this exact defect, but neither was exercised by a failing gui-smoke spec, so this ticket doesn't
claim to have proven it live. Worth a quick audit + the same two-line fix next time either is touched.

## Verification
Not confirmed on a live CI run (this session cannot run `tauri-driver`/WebKitGTK locally — see CPE-1595's
Notes). `gui-smoke/known-failing.json`'s `shred-dialog.smoke.ts` entry is left in place (not deleted)
until a real Linux CI run confirms the spec passes; see that entry's updated `reason`.
