---
id: CPE-1977
title: the launcher's two inline colour palettes — session-identity chips (fail white-text contrast, and the same array drives the main app's Agents leaf) and STATE_META's status dots
type: bug
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-28
---

## Summary

Found by CPE-1966's new real-browser contrast sweep
(`scripts/dev-harness/launcher-contrast/`), which mounts one chip per entry of the launcher's own
`SESSION_CHIP_COLORS` array — read out of `launcher.html` at run time rather than sampled — and
measures each one both ways.

The chip is a 16px rounded square carrying a white numeral at **10px / weight 700**. That is
"normal" text for WCAG 2.1 (large starts at 18.66px bold), so it needs **4.5:1**, and the chip's
own fill needs **3:1** against the tab it sits on. Measured (headless Chrome over CDP,
`getComputedStyle` + a screenshot cross-check agreeing to within 1/255):

| pairing | measured | bar |
|---|---|---|
| white numeral on `#2aa1a1` | **3.13:1** | 4.5 |
| white numeral on `#3a9d4a` | **3.44:1** | 4.5 |
| `#2aa1a1` fill on an inactive tab, light | **2.61:1** | 3.0 |
| `#2aa1a1` fill on a hovered tab, light | **2.42:1** | 3.0 |
| `#3a72b5` fill on a hovered tab, dark | **2.87:1** | 3.0 |

`sessionColor()` picks by hash, so any session can land on any of these — this is not an edge case,
it is one-in-eight per session.

## Why CPE-1966 measured it but did not fix it

Two reasons, both worth keeping straight:

1. **It is not in the stylesheet the sweep guards.** The colour arrives as
   `chip.style.background = sessionColor(id)` — an inline style. CPE-1966's harness reports every
   such reading under "MEASURED, NOT ENFORCED" precisely so the number is never mistaken for zero,
   but it does not red on them.
2. **The array is duplicated in the main app.** `src/lib/sessionChip.ts` declares the same eight
   values and drives the explorer's left-pane Agents leaf, deliberately (CPE-490: same colour and
   number in both surfaces). Changing one without the other breaks that; changing both is an
   app-wide visual-identity decision with its own tests (`src/lib/sessionChip.test.ts`), not a
   line-item inside a launcher contrast fix.

## The second palette: `STATE_META`'s status dots (added in CPE-1966's round-2 review)

This ticket originally said "the session-chip palette", singular. There are **two** inline palettes
in that one file, and the second one is worse off than the first because the harness does not even
*report* it.

`STATE_META` (`launcher.html`) assigns `.state-dot`'s background inline in `renderState()`:
`#d08a1a` blocked / `#3a72b5` working / `#3a9d4a` done / `#7a7a7a` idle. CPE-1966's fixtures mount
`.state-dot` but never run `renderState()`, so the harness measures the CSS default `#7a7a7a`,
drops it as non-chromatic, and prints **nothing** — not a failure, and not a line under "MEASURED,
NOT ENFORCED" either. Measured by hand:

| pairing | measured | note |
|---|---|---|
| `#d08a1a` dot on a light tab | **2.38:1** | the same number that made CPE-1966 retire this hex from `.tab.blocked` |
| `#3a9d4a` dot on a light tab | **2.86:1** | the hex CPE-1966 retired from `.badge.yes` |

**Not a hard SC 1.4.11 failure**, and that is why it is scoped here rather than fixed in CPE-1966:
each dot carries a `title=` ("Agent blocked" / "working" / "done") and the grid pane's `.pane-state`
spells the same word out in text, so colour is not the only carrier of the information. But two of
the four values are hexes this repo has already decided are too weak to carry meaning on their own,
and they are unmeasured by the sweep that is supposed to see everything.

## Acceptance criteria

- [ ] Re-tune `STATE_META`'s four values against both tab grounds in both schemes, to the same 3:1
      the rest of the launcher's chromatic non-text is held to.
- [ ] Give CPE-1966's harness a fixture that actually exercises them, so they stop being invisible:
      either mount `.state-dot` with each `STATE_META` colour applied inline (derived from the array
      in `launcher.html`, the way `sessionChipColours()` already derives the chip palette — never
      copied), or have `renderState()` be callable from the fixture. Then the numbers appear in the
      report and this class of miss cannot recur silently.
- [ ] Re-tune the eight values so the white numeral clears 4.5:1 on every one of them, and every
      fill clears 3:1 against **both** tab states in **both** schemes (the hovered tab is the harder
      ground in light: `#e2e2e2`, not `#eaeaea`).
- [ ] Keep the eight visually distinct — the palette's job is identity. Check hue separation the way
      `aiConsoleLauncher.contrast.test.ts` already does for the three status colours.
- [ ] Change **both** copies in lockstep (`sidecar/ai-console/src/launcher.html`'s
      `SESSION_CHIP_COLORS` and `src/lib/sessionChip.ts`) — or better, decide whether one can read
      the other, since two hand-kept copies of one palette is the CPE-1933 shape.
- [ ] Re-run `npm run harness:launcher-contrast` and confirm the "MEASURED, NOT ENFORCED" block
      empties; then consider whether the inline-style exemption in that harness's `enforced()` can be
      narrowed now that the palette is compliant.

## Notes

Filed 2026-08-28 from CPE-1966's sweep. The numbers above come straight out of
`npm run harness:launcher-contrast` and are reproducible by running it.
