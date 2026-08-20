---
id: CPE-1827
title: the Trash titlebar cannot fit seven buttons and a title on one line at supported widths
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

`.tv-titlebar` puts a title, a count/status slot, and seven buttons on one unwrapped line. Below
roughly 880 px that does not fit, and the failure is silent: `.tv-tools` overflows under
`overflow: visible` and `.tv-panel { overflow: hidden }` cuts the remainder off. Measured on `main`'s
markup (round-1 geometry in the CPE-1816 review, streaming state):

- **≤684 px** — `.tv-tools` overflows the panel by 106 px. "Delete selected permanently", "Empty
  Trash", refresh, Docs and the close **×** are all clipped or gone.
- The app's own floor is **600 × 400** (`src-tauri/src/lib.rs`, `.min_inner_size`), so this whole band
  is a size the app explicitly permits.

**The close button is the serious part.** There is no Escape handler in `TrashView` (verified), so once
**×** is clipped the only remaining way out is the ~2vw backdrop strip that `.tv-overlay`'s
`on:click|self` catches. A modal whose only exit is a sliver of backdrop is a trap.

CPE-1816 twice tried to solve a *related* symptom inside its own scope and each attempt moved the
damage: a `min-width: 0` on the title let the title (and the loading caveat) collapse to nothing, and a
`min-width: 34ch` floor pushed the toolbar off the edge at 700–880 px — a band that had previously been
fine. Both were reverted. The real cause is density, not a `min-width` value.

## Acceptance criteria

- [ ] Pick and record ONE approach. The Visual Critic's recommendation is an **overflow "…" menu**,
      because it is the only option of the three that keeps the close button on the first line at every
      supported width. The alternatives considered were icon-only buttons below a breakpoint, and
      letting the bar wrap onto a second row.
- [ ] At every width from 600 px up, in all three listing states (streaming, complete, degraded) and with
      and without a selection: the close **×** is present and hit-testable, and no control is silently
      clipped.
- [ ] An **Escape** handler closes the view, so keyboard users are never dependent on a visible ×. Check
      the repo's other overlays first and match whatever convention they already use.
- [ ] Verify in all 12 locales — several are materially wider than en-US, and the CPE-1816 measurements
      showed Russian is the worst case for the status slot.
- [ ] Whatever ships is pinned by the Trash gui-smoke spec from CPE-1822 (hit-test the × at a narrow
      width), not by a jsdom structural assertion — jsdom does not compute layout, which is exactly why
      three rounds of this went unguarded.

## Notes

Filed from the CPE-1816 Visual Critic's finding 5, which it raised in round 1 and re-measured in every
round after. The ≤684 px overflow is **pre-existing**, not caused by CPE-1816. Related: CPE-1822 (no
gui-smoke coverage of the Trash view at all) is a prerequisite for pinning any of this properly.
