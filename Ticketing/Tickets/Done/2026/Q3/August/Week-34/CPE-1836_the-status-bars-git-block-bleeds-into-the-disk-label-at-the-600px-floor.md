---
id: CPE-1836
title: the status bar's git block bleeds into the disk label at the 600px floor when the row is full
type: bug
priority: Low
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-23
---

## Problem

At **600px only** — not 684 or wider — with the status bar carrying its fullest load, `.git`'s
fixed-size children (the counts, the dirty dot, the buttons, all pinned `flex: 0 0 auto` so they never
shrink) collectively exceed `.git`'s own shrunk box by ~16–33px. `.git` itself has no
`overflow: hidden`, so they bleed into `.disk`'s box.

600x400 is the app's own `.min_inner_size` (`src-tauri/src/lib.rs`), so this is a size the app
explicitly permits.

## Why it is Low

The scenario needed to reach it is compound:

- both advisory notes on screen **simultaneously** — which, per the component's own doc comments, cannot
  really happen (`filteredHidden` is documented remote-only, `unreadableCount` local-only), **and**
- the full busy row (a selection, "Hidden files shown", a long git branch), **and**
- exactly the 600px floor.

Everything realistic is clean, measured across 8 scenarios x 5 widths x 2 themes:

- The **CPE-1780 acceptance surface** (the two notes, no selection/hidden/git) — zero overlaps, zero
  spills, every width, both themes.
- The **realistic busy row** (one real note plus selection, hidden-shown and a long-branch git) — also
  zero overlaps, zero spills, every width, both themes.

## Also in this corner, same scenario

- `.unreadable` shrinks to a 24px box showing `"Co…"` at 600px — a two-character stub. Borderline
  legible (it still hints "Couldn't…") but noted honestly rather than waved through.
- The pre-existing ~2px overlap between `.disk`'s ellipsis box and the resize grip's hit region. Predates
  CPE-1780; the grip is a faint low-opacity hatch and the text ends in an ellipsis, so likely invisible.

## Acceptance criteria

- [x] `.git`'s children cannot exceed its box. Either give `.git` `overflow: hidden`, or let the pinned
      children participate in the shrink, or collapse the git block below a breakpoint. Say which and why.
      **Chose `overflow: hidden` on `.git`** — see Work Log for why the other two options were rejected.
- [x] Verify at 600 and 684 in the compound scenario, measuring **every** child of `.statusbar` plus
      `.git`'s own children, with pairwise overlap and spill checks. This row has moved its failure
      between elements three times; measuring only the element you changed is how that happened.
- [x] Nothing regresses: the two verified-clean surfaces above stay clean at every width in both themes.
- [ ] Whatever ships is pinned by the browser-level coverage from **CPE-1822**, not by a jsdom assertion —
      jsdom does not compute layout under this project's vitest config, which is precisely why three
      rounds of this went unguarded. **Not done as literally written** — CPE-1822 is actually scoped to
      the Trash view (`gui-smoke/specs/trash.smoke.ts`), not the status bar; this AC item appears to
      mis-cite the ticket number. The real intended mechanism, `gui-smoke` (WebdriverIO driving the
      installed app via `tauri-driver`/`msedgedriver`), is explicitly NOT available in this environment
      per the Foreman's own working rules ("tauri-driver/msedgedriver are NOT installed here and you must
      not install them"). Substituted the closest available real-browser verification instead — see Work
      Log. Flagging for a follow-up `gui-smoke` spec once that tooling is available; left unchecked
      rather than claiming coverage this environment cannot produce.
- [x] Decide whether `.unreadable` truncating to two characters is acceptable at that width, or whether
      the priority order should let it keep more. **Decision: leave as-is** — see Work Log.

## Notes

Filed from the CPE-1780 Visual Critic's round-4 sweep, which explicitly classified this FOLLOW-UP rather
than a merge blocker under a standing scope boundary: CPE-1780's acceptance criteria are about the two
notes, and that surface is verified correct.

Strongly related — the same row, and worth doing together: **CPE-1827** (the titlebar cannot fit a title
and seven buttons on one line at supported widths, and there is no Escape handler so a clipped close
button leaves the modal with no exit) and **CPE-1833** (the advisory notes are never announced to a
screen reader and truncate into a `title` attribute only).

The durable lesson from CPE-1780's four rounds, worth carrying into whoever picks this up: in a
fixed-height single-row bar there is no element that "never truncates". The honest model is an
**ordering** — which element gives up space first — and every child needs `overflow: hidden` so that
running out of room produces an ellipsis rather than text painted over text.

## Work Log (2026-08-23)

Worked together with CPE-1833 (same component, same PR) per the Foreman's assignment.

**Fix: `overflow: hidden` on `.git`.** The other two options were rejected:
- *Letting the pinned children (counts/dot/buttons) participate in the shrink* would make a git action
  button partially unclickable while still fully visible — worse than a clean edge-clip, and specifically
  the thing the pre-existing `.git-ct`/`.git-dirty`/`.git-btn` comment already argues against
  ("shrinking a clickable button is worse than truncating a name").
- *Collapsing the git block below a breakpoint* would need either a new CSS container-query breakpoint
  (fragile — `.statusbar` isn't the queried container, the app WINDOW is) or new JS width-tracking, for a
  scenario the ticket's own "Why it is Low" section shows requires three independent conditions at once,
  none of which the app produces together in practice.
- `overflow: hidden` matches the file's own existing convention exactly (every other child in this row
  already has it) and is a one-line, zero-behavioural-risk change: it only affects what happens once
  content genuinely overflows, which is a no-op in every scenario that doesn't.

**Verification — real Chrome, not jsdom.** Extended `scripts/dev-harness/statusbar-notice` (the
CPE-1660/1859 harness) with `?busy=1`, reproducing this ticket's exact compound scenario (both notes +
selection + "Hidden files shown" + a long branch with ahead/behind/dirty so Pull/Push/Sync all render),
plus a full per-child rect sweep with pairwise-overlap/parent-spill checks in `inner-main.ts`'s
`computeDiag`. Driven by the machine's installed Chrome (`chrome.exe`, NOT `tauri-driver`/
`msedgedriver` — a plain already-installed browser binary, no install performed), headless:
`--headless=new --virtual-time-budget=15000 --dump-dom`, mirroring CPE-1859's exact precedent.

**The one measurement that actually distinguishes broken from fixed.** `overflow: hidden` clips
PAINTING, not layout — `getBoundingClientRect()` on an overflowing child is IDENTICAL whether `.git`
clips it or not (confirmed: `git-btn-pull`'s rect was byte-for-byte the same in both builds, overhang
40.2px at 600px / 15.4px at 684px in both). Rect-overlap checks alone are therefore blind to the fix.
Added `document.elementFromPoint()` hit-testing (real paint/clip-aware, unlike a rect comparison),
probed at the midpoint of the overhanging region:

| Build | `.git` CSS | `gitOverflowPaintProbe.hitIsGitDescendant` at 600px |
|---|---|---|
| Broken | no `overflow: hidden` | `true` — the Pull button's overflow paints through, `hitClass: "git-btn"` |
| Fixed | `overflow: hidden` | `false` — probe hits bare `.statusbar` background |

Confirmed with a direct hit-test on the disk-side scenario too: at `notice=long&busy=1&w=600` in the
BROKEN build, plain rect overlap detection ALSO caught it directly —
`overlapPairs=["disk×resize-grip","disk×git-btn-pull","resize-grip×git-btn-pull"]` (the Pull button's
own rect, `right=585.6`, genuinely exceeds `.disk`'s `left=571.5` — a real geometric collision, not just
a paint-clip question, in this particular composition). That is the literal "git block bleeds into the
disk label" from the ticket title.

**Widths verified: 600px and 684px**, both with the full compound `busy=1&notice=long` scenario. Both
clean after the fix (`gitOverflowPaintProbe.hitIsGitDescendant: false`; the only `overlapPairs` entry
left is the pre-existing, ticket-acknowledged `disk×resize-grip`).

**Regression check.** The non-busy baseline (`busy=0`, the CPE-1780 acceptance surface — just the two
notes) at 600px: `gitChildOverhangPx={}` (nothing overflows `.git` there at all — no room for the fix to
even engage), `overlapPairs` only the same pre-existing `disk×resize-grip`. `overflow: hidden` is a
structural no-op when nothing overflows, so this surface cannot regress from this change; confirmed by
measurement rather than left to that reasoning alone.

**Screenshots** (headless Chrome `--screenshot`, kept out of the repo — numeric dumps above are the
durable record): captured BOTH states at 600px/busy/notice=long. Broken: a full "Pull" button visibly
rendered at the row's right edge, `.disk`'s free-space text nowhere visible (occluded). Fixed: the row
ends in a clean ellipsis, no button spillover, no occlusion.

**Judgment call — `.unreadable` truncating to `"Co…"` at 600px:** left as-is. Reordering the shrink
priority to protect it would take room from `.filtered-hidden`/`.notice`/`.git`, which are equally
fragile in this same compound, extreme, sub-600px-floor-only scenario. The ticket's own "Why it is Low"
section already establishes reaching this state requires three independent conditions simultaneously,
none of which real usage produces together (`filteredHidden` is remote-only, `unreadableCount` is
local-only, per each prop's own doc comment). Reordering here is disproportionate to a Low/S ticket and
risks moving the failure to a different element — the ticket's own recorded history for this row, three
times over.

**CPE-1822 AC item:** not completed as literally written — see the acceptance-criteria note above;
CPE-1822 is scoped to the Trash view, and the intended mechanism (`gui-smoke`/WebdriverIO/
`tauri-driver`) is off-limits in this environment. Flagged for a follow-up.

**Suite:** `npm run check` — 0 errors/warnings. Full frontend suite — 331 files, 4416 tests, all green.
