---
id: CPE-1977
title: the launcher's two inline colour palettes — session-identity chips (fail white-text contrast, and the same array drives the main app's Agents leaf) and STATE_META's status dots
type: bug
priority: Medium
status: Done
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

## Closing record — merged as PR #1102 (`69450ca4`), 2026-08-28

Every number below came out of `npm run harness:launcher-contrast` (headless Chrome over CDP), re-run after
each change, and the Reviewer **re-ran the harness on the base revision too**, so the "before" column is
measured rather than quoted.

### Palette 1 — the session chips, which failed a *text* bar

The chip is a 16px rounded square carrying a **white numeral at 10px / weight 700**. That is **"normal"
text** for WCAG 2.1 (large starts at 18.66px bold), so it needs **4.5:1**; the fill needs **3:1** against
the tab. Those two bars leave **one narrow luminance window** — `L ≤ 0.1833` for white at 4.5, `L ≥ 0.1725`
for the dark hovered tab at 3:1 — and all eight now sit at its midpoint.

| | before | after |
|---|---|---|
| white numeral, worst | **3.13:1** | **4.57:1** |
| chip fill, worst | **2.42:1** (hovered light tab) | **3.08:1** |
| min pairwise hue separation | **10.5°** | **34.6°** |
| min CIELAB dE76 | 18.6 | **30.3** |

**The ticket listed three failing colours; seven of the eight were failing** — the harness's dedup keeps
only the worst reading per site, so the report was showing a third of the problem. `sessionColor()` picks
by hash, so this was **one-in-eight per session**, not an edge case.

**Both distinctness metrics improve**, which is the guard against buying contrast by collapsing the
palette — the failure mode where eight indistinguishable chips pass the test and fail the feature.

### Palette 2 — the status dots, which the harness did not even *report*

`renderState()` assigns `.state-dot`'s background inline. **CPE-1966's fixtures mounted `.state-dot` but
never ran `renderState()`**, so the harness measured the CSS default grey, dropped it as non-chromatic, and
printed **nothing** — not a failure, and not a line under "MEASURED, NOT ENFORCED". Two of the four values
were hexes this repo had already retired from other roles: **2.38:1** and **2.86:1** on a light tab.

Retuned to `#9f7218` / `#2477e9` / `#158d41` / `#7a7a7a`, worst reading across both tab states, both
schemes and the pane head **3.30–4.21:1**. They **cannot** be scheme-keyed the way `--blocked-bar` is,
because the dot lands on a scheme-following tab **and** on `.pane-head`, which is `#161616` in both
schemes — that constraint is what sets their luminance.

**A second trap in the same dedup, found on the way and worth more than the retune.** All `.state-dot`s on
a `.tab` shared one `path` key, and the dedup keeps the **worst**, so the neutral default won and was then
dropped as non-chromatic. **It was not merely hiding the chromatic dots from the report — it was taking
them out of enforcement.** Re-measured for the record: deleting the `for-<state>` class split drops the
inline population **30 → 16**, exactly one chromatic dot survives per scheme, **both `.tab:hover` readings
revert to the neutral default and are dropped** — so the *harder* tab ground, the one these colours were
retuned against, ends up enforced for **zero of the four states**, and the harness **exits 0**.

### The `enforced()` exemption — deleted, not narrowed

Its *"the palette is shared, so this is an app-wide call"* reason **expired with this ticket**; its *"not
authored in the stylesheet"* reason was about **where a colour is authored, not what a user sees**. The
fragility objection was measured rather than argued: base THIN MARGINS is **28 readings in 7 rules, of
which 23 are at +0.05**; head is 35 in 14 rules — the same 28 plus **7 new inline readings at +0.07 (×4)
and +0.08 (×3)**. **Every newly-enforced inline reading is looser than all 23 pre-existing ones**, so
deleting the exemption widens coverage at a margin the job already accepts rather than raising flake risk.

Result: **30 inline sites, 26 enforced, 0 under bar.** `MEASURED, NOT ENFORCED` is replaced by a counted
`INLINE-ASSIGNED` block that prints pass **or** fail, plus a new **`legsThatDidNotRun` floor so an empty
inline population reds instead of reading as clean** — the exact state the status dots spent CPE-1966 in.
Sabotaged by the Reviewer (every inline background stripped): **exit 1, `INLINE-ASSIGNED — 0 site(s)`**.
It fires.

### The guard that could not fire for its own subject

The first "retired hex" guard stayed **green 20/20** against `#d08a1a` — **the very colour it was named
after** — because that hex is still `--blocked-bar`'s **dark** value, so *"a hex the stylesheet no longer
declares"* was false for it **by construction**. Rewritten to difference the two resolved token maps; it
now reds naming the state. **The dead version is recorded at the site**, and this is the worked example
carried into CPE-1985: *a guard named after a value must be tested against that value.*

### Both copies, pinned by derivation

Kept as two copies — `launcher.html` is a standalone document with no module graph into `src/`, and the
harness **derives** the palette by parsing it, so a placeholder would remove the ground from the only
browser measurement. `sessionChip.test.ts` now parses the launcher's array **with the harness's own
parser** and asserts equality **including order**, with a red-proof that mutates the source every run
(Reviewer's own sabotage: swapping two entries reds).

### Visual verdict — `VISUAL PASS`, with three costs recorded

Rendered at real 16px / 10px-700 geometry on all four tab grounds, plus 4× zoom and old-vs-new.

**The old palette's two genuine collisions are gone** — blue vs indigo (dE 18.6; at 4× they read as one
blue) and amber vs orange (10.5°). It reads as a **deliberate palette**: all eight pinned to **L\*
49.3–49.4** where the old ranged 47.3–60.4, *"and that isoluminance is what makes the set cohere rather
than look like filter survivors."*

Three costs, recorded rather than discovered later:

- **Olive vs brown is the tightest pair** — tellable at 16px, but the one to watch.
- **Magenta and violet read more vivid** than the other six, because saturated blue-purples hold chroma at
  that luminance where olive/teal/brown do not. **Unfixable without breaking the window.**
- **`blocked` loses a little salience** at 8px on the fixed-dark pane head. Blocked is the state that most
  needs to draw the eye; the `dotpulse` animation still carries it, and the note says the salience must
  come back from **motion, size or a ring, never from lightening the hex** — which is pinned between the
  two tab grounds with no room either way.

**And one thing recorded so it is never mistaken for a regression from this PR:** a tab shows a dot and a
chip 8px apart, and the new done-dot vs chip #2 is dE76 **8.9** / 0.4° — they partially merge. But the
**old** palette used **literally identical hexes** for both roles, twice (dE76 **0.0**). This PR improves a
pre-existing overlap rather than introducing one.

### Standards

The diff adds **zero CSS declarations**, so there is no `--accent` / `--accent-text` exposure at all,
including the `var(--accent, <fallback>)` spelling that hid five of seven sites in a prior sweep. Tick-tack
rule untouched and not violated: `#tabs` scrolls per `docs/design/TABS.md`, `.tab` is `white-space:
nowrap`, `.tab-chip` and `.state-dot` are both `flex: 0 0 auto`. **No `.agent-chip`/`.menu-chip` ratio
crept in anywhere** — the Reviewer grepped every ratio the diff introduces.

### A standing convention left declared rather than silently violated

`launcher.html`'s CPE-1921 comment says the state should pick a **class**, and the class a token, *"never
an inline hex … which can't be checked against the ground it lands on."* `STATE_META` still paints inline,
one line from a `renderState()` call that **already toggles a class**. The rebuttal is correct for the
comment's **first** half (these four cannot be scheme-keyed) and **not** for its second — this PR bought
checkability with harness machinery instead. **Declared as the known exception at both ends**, with the
refactor filed rather than implied.

### Filed, not fixed

**CPE-1985** — the main app's `.agent-chip`/`.menu-chip` on `--surface`/`--bg`/`--hover` across four themes
are swept by **nothing**; the values cannot drift (pinned) but the grounds are unmeasured. **No ratio for
them appears anywhere in code**, deliberately, since that would be a claim with no measurement behind it.
The ticket also carries the `STATE_META` refactor and two concrete unswept sites the Reviewer found —
including a component hard-coding **two hexes this PR just retired**.

### Gates at merge

`npm run check` **0 errors / 0 warnings** · vitest **362 files, 5,444 passed / 2 skipped** · harness
`--verify-pixels` **exit 0** (30 inline sites, 26 enforced, 0 under bar; 59 verified, 0 disagreeing, 8
UNVERIFIED per scheme — the pixel cost of four extra tabs, declared) · CI `completed success —
total_count=26 pending=0 skipped=1 coverage=ok`.

**Unseen, and said so:** the running app. A launcher change needs the **host** rebuilt — a launcher swap is
not a host swap — so window chrome and any WebView2 layout difference remain unverified. Everything above
is the real-browser harness over the real `launcher.html`, the vitest suite, and an off-screen render of
both palettes at real geometry.

**Family:** CPE-1966 (the real-browser contrast harness, and where the dots hid), CPE-1985 (the app-side
sweep this declares as missing), CPE-490 (same colour and number in both surfaces — why two copies exist),
CPE-1919 (`--accent` fills vs `--accent-text` reads; a token backing several roles gets pinned at the
loosest bar), CPE-1921 (the class-and-token convention this declares an exception to), CPE-1933 (derive
provenance, don't claim it), CPE-1950 (remove removable duplication).
