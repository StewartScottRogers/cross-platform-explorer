---
id: CPE-1836
title: the status bar's git block bleeds into the disk label at the 600px floor when the row is full
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
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

- [ ] `.git`'s children cannot exceed its box. Either give `.git` `overflow: hidden`, or let the pinned
      children participate in the shrink, or collapse the git block below a breakpoint. Say which and why.
- [ ] Verify at 600 and 684 in the compound scenario, measuring **every** child of `.statusbar` plus
      `.git`'s own children, with pairwise overlap and spill checks. This row has moved its failure
      between elements three times; measuring only the element you changed is how that happened.
- [ ] Nothing regresses: the two verified-clean surfaces above stay clean at every width in both themes.
- [ ] Whatever ships is pinned by the browser-level coverage from **CPE-1822**, not by a jsdom assertion —
      jsdom does not compute layout under this project's vitest config, which is precisely why three
      rounds of this went unguarded.
- [ ] Decide whether `.unreadable` truncating to two characters is acceptable at that width, or whether
      the priority order should let it keep more.

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
