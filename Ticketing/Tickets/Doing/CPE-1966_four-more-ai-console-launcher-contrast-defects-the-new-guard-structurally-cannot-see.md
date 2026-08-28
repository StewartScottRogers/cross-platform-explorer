---
id: CPE-1966
title: four more AI Console launcher contrast defects — including the **only focus indicator** at 2.46:1 — that CPE-1921's new guard structurally cannot see
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Found by PR #1076's Reviewer, which built its **own** measurement harness rather than trusting the
PR's — a from-scratch PNG decoder, its own WCAG 2.1 implementation, and two independent paths (engine
`getComputedStyle` and sampled pixels) agreeing within 1/255. It reproduced all twelve of the PR's
ratio cells exactly, then found four defects the PR's sweep reported as absent.

All four are **pre-existing** — CPE-1921's diff is clean. What they have in common is more useful than
the individual numbers: **each is invisible to the new guard for a structural reason**, so the sweep
honestly reported zero.

| # | site | measured | bar | why the guard cannot see it |
|---|---|---|---|---|
| 1 | **`select:focus` / `input:focus` / `textarea:focus` border** vs `Field` (`rgb(59,59,59)`), dark | **2.46:1** | 3:1 (WCAG 1.4.11) | `SITES` derives from `color: var(--token)`; this is `border-color`, a **non-text** role |
| 2 | `#swarm-help` / `#grid-help` (`.area-help`, flat `opacity:.75`, 13px/700), dark | **4.24 / 4.28** | 4.5 | `#view-bar` is `display:none` on the static page the sweep loads |
| 3 | `.boot-label` (`boot-pulse` swings opacity .45↔.85), light | dips to **3.35** | 4.5 | an **animated** opacity; the sweep samples one static frame |
| 4 | `.close-all-btn:hover { color: #d05656 }`, 11px, light | **4.08:1** | 4.5 | invisible **twice** — a literal hex, and a `:hover` state the derivation does not model |

**#1 is the one to fix first.** `outline: none` means that border is the **only** focus indicator on
every select, input and textarea in the AI Console. At 2.46:1 in dark it is below the 3:1
non-text-contrast bar, so keyboard focus is hard to locate — a keyboard-navigation defect, not a
cosmetic one.

**#4 is quietly ironic:** `#d05656` is the very hex `keysMsg` abandoned in CPE-1921 for failing its
bar. It survives on a `:hover` rule the sweep does not reach.

## Why the sweep honestly reported zero

The PR's sweep loaded the **static** launcher page. There, `#view-bar` is `display:none` and a
`position: fixed; inset: 0; z-index: 9999` boot overlay covers everything — so sites 2 and 3 were
either not rendered or not reachable, and sites 1 and 4 need a **state** (`:focus`, `:hover`) the
derivation does not model.

That is the interesting part and the reason this is its own ticket rather than a rider: **a contrast
guard that only samples the default state of the default view will report zero on a page whose
defects live in states and in hidden panels.** The fix is not four colour changes — it is a sweep that
enumerates states and forces panels visible.

## Acceptance criteria

- [ ] **Fix site 1 first and separately.** It is a keyboard-accessibility defect in the only focus
      indicator. Decide whether `--accent` is the right token for a focus ring at all, or whether the
      focus role wants its own token — CPE-1921 already split the **text** role out of `--accent` for
      exactly this reason, and this is the same shape one role over.
- [ ] **Measure every site before and after**, with the tool named, on the real page. **Do not estimate
      and present as measured** — that was PR #1069's round-2 blocker, twice.
- [ ] **Extend the guard to reach states and hidden panels**: `:hover`, `:focus`, animated opacity
      (sample the extremes, not one frame), and `display:none` panels forced visible. **This is the
      deliverable**, more than the four fixes.
- [ ] **Cover non-text roles.** `SITES` derives from `color: var(--token)` only, so `border-color`,
      `background-color` and `box-shadow` roles are unpinned. Add them at the **3:1** non-text bar,
      and keep each role at **its own** bar rather than the loosest — CPE-1921 and CPE-1919 both found
      `--accent` multi-role and both had to split it.
- [ ] **Cover literal hexes**, or state plainly that the guard is token-only and that a literal hex
      walks straight through it. Site 4 is invisible for that reason alone.
- [ ] **Re-derive the whole sweep afterwards and report the real number**, with the states it now
      covers and the ones it still cannot. A qualified number beats an unqualified zero.
- [ ] Also check the `#help-panel`/`#keys-panel` ancestor-background assumption: the "no checked
      element paints its own background" test inspects only the element's own rule and only the **first**
      matching rule, so if `#help-panel` stopped painting `Canvas` the ground would move silently.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1076's Reviewer, which measured rather than agreed.
CPE-1921's own code is correct and its ratio table survived an independent re-measurement of all
twelve cells; these are pre-existing defects its sweep structurally could not reach.

**Related, and worth reading together:** **CPE-1921** (the fix, PR #1076 — and the sweep to extend),
**CPE-1919** / **CPE-1962** (the app-side `--accent` split and the theme-parity guards — same
multi-role trap, same measure-don't-estimate rule), **CPE-1933** (a claim that reads as vouched-for
because a green test sits beside it).

**Sidecar caveat:** the launcher is `include_str!`'d into the host, so a real-app check needs the
**host rebuilt with sidecar config** — a launcher swap is not a host swap.

## Two corrections to site 4, measured on PR #1076 round 2

Re-derived independently while correcting #1076's claims (headless Chrome `--dump-dom` over the real
`<style>` blocks and `<body>` markup, `:hover`/`:focus` forced on by stripping those pseudo-classes
from a second copy of the sheet, and the JS-built `.close-all-btn` injected into the real `#tabs`):

- **Site 4 fails in dark too — 4.13:1** — not only in light. The row above reads as light-only.
- **Its light figure is 3.65:1, not 4.08.** 4.08 is `#d05656` on bare white; the button actually sits
  inside `#tabs`, which paints `rgba(128,128,128,0.10)` → `rgb(242,242,242)` in light. Worse, not
  better, and the same class of mistake the ticket is about: a colour measured against a ground it
  does not land on.

Sites 1, 2 and 3 re-measured to **2.46 / 4.24 / 4.28 / 3.35** exactly as recorded.

## Work Log

### 2026-08-28 — the sweep first, then the fixes

**The tool, named, and validated before it measured anything.**
`scripts/dev-harness/launcher-contrast/` — installed Chrome driven over raw CDP (`--headless=new`,
Node's built-in `WebSocket`, no WebDriver and no npm dependency), the same shape
`scripts/dev-harness/layout-guard/` proved out for CPE-1882. `validateAnchors()` runs first and
refuses to continue unless this implementation reproduces `#000/#fff = 21.00`,
`#767676/#fff = 4.54`, `#949494/#fff = 3.03`, plus two **compositing** anchors: 50% black over white
= **3.98** (it caught my own first draft, which had 3.95 written from memory), and a check that the
text-only and both-dimmed opacity models give *different* answers — the exact mistake CPE-1921's
round 2 made when it first reported site 2 at 3.81.

**A second, independent measurement path.** `--verify-pixels` screenshots each scheme, decodes the
PNG with a from-scratch decoder in the harness, and compares the painted ground at each site against
what the computed-style path predicted: **59 grounds per scheme, 0 disagreeing by more than 1/255.**
(The first sampler read one corner pixel and reported a badge's `#3a9d4a` fill as `#d2e5d5` — an
antialiased edge. It now takes the mode of a 9×5 interior grid.)

**Engine-resolved system colours, measured this run:** light `Canvas` rgb(255,255,255), `CanvasText`
rgb(0,0,0), `Field` rgb(255,255,255), `ButtonFace` rgb(240,240,240); dark `Canvas` rgb(18,18,18),
`CanvasText` rgb(255,255,255), `Field` rgb(59,59,59), `ButtonFace` rgb(107,107,107).

### The four sites, before and after

| # | site | before | after | bar |
|---|---|---|---|---|
| 1 | `select:focus` border vs the `Field` interior, dark | **2.46:1** | **3.84:1** | 3.0 |
| 1 | …and vs the page outside it, light / dark | 4.21 / 4.12 | **4.21 / 6.42** | 3.0 |
| 2 | `.area-help` (`#grid-help` / `#swarm-help`), dark | **5.07 / 4.94** here; 4.24 / 4.28 as filed | **5.33 / 5.33** | 4.5 |
| 3 | `.boot-label` at its animation trough, light | **3.35:1** | **6.20:1** | 4.5 |
| 4 | `.close-all-btn:hover`, light / dark | **3.65 / 4.13** | **5.17 / 5.91** | 4.5 |

Sites 1, 3 and 4 reproduced to the last digit, including the ticket's own in-place correction of
site 4 (3.65 light / 4.13 dark, on `#tabs`'s `rgba(128,128,128,0.10)` fill rather than bare white).

**Site 2 did not reproduce, and that is itself the finding.** `.area-help` is `ButtonText` on a
`ButtonFace` fill under a blanket `opacity: .75`, so its ratio is decided by whatever the engine
resolves `ButtonFace` to in dark. This Chrome gives rgb(107,107,107) → **5.07:1, passing**; the
4.24/4.28 in the ticket implies a build resolving it near rgb(120,120,120). Both are correct
measurements of different browsers. A colour whose compliance is decided by a browser build is not a
colour we control, so the blanket opacity is **gone** rather than retuned — the same call CPE-1921
made for `#msg`, one site over. The hover cue is now a fill, not a transparency.

### Five more defects the sweep found that the ticket did not list

All stylesheet-authored (so enforced), none visible to a token-driven static parse:

- **`.pane-head` and its three labels: 1.09 / 1.10 / 1.10:1 in LIGHT** — the worst readings on the
  page. It hard-codes `background: #161616` (the grid view sits on a permanently dark terminal
  surface) while inheriting `CanvasText`: black on near-black in light theme. Invisible to CPE-1921
  three ways over — a literal hex, a non-text role, and an element that only exists once JS builds
  the grid view. Now sets `color: #e6e6e6`.
- **`.tab { color: GrayText }`** — the system colour for *disabled* text, on tabs that are one click
  from active. `.tab-label` 4.31 (light) / 3.93 (dark); `.tab-close` 2.57 / 2.64, and **3.24 while
  hovered**, because the hover fill *lightens* the ground under it. Now `--tab-text`.
- **Three `opacity` dimmers stacked on that already-dim colour** — `.tab-usage` (.62 → .88),
  `.tab.ended .tab-label` (.6 → .9), `.tab-close` (.7 → removed). Opacity multiplies losses.
- **`.badge.no` 3.25:1 and `.badge.yes` 3.44:1** under their own literal `#fff` at 10px/600 — normal
  text for WCAG, so a 4.5 bar, not 3. Now `--badge-warn` / `--badge-ok`.
- **`.tab.blocked`'s `inset 2px 0 0 #d08a1a` bar: 2.39:1 light, 2.22:1 on a hovered tab.** Now
  `--blocked-bar`, with its own light value.
- **`.sw-empty { opacity: 0.5 }` → 3.98:1 light.** Raised to .72.

### The re-derived sweep, after the fixes

```
1,306 raw readings -> 786 distinct sites, 384 enforced
light: 53 forced pseudo-state readings, 3 CSS animations x 21 frames, 59 grounds pixel-verified (0 disagreements)
dark:  53 forced pseudo-state readings, 3 CSS animations x 21 frames, 59 grounds pixel-verified (0 disagreements)
PASS - every enforced site clears its bar in both schemes.
MEASURED, NOT ENFORCED - 9 readings under bar (the shared session-chip palette)
```

The nine unenforced readings are the honest remainder: `chip.style.background = sessionColor(id)` is
an inline colour from an array duplicated in `src/lib/sessionChip.ts`, which also drives the
explorer's Agents leaf. Worst are white-on-`#2aa1a1` at **3.13:1** and the `#2aa1a1` fill at
**2.42:1** on a hovered tab. Filed as **CPE-1977** rather than changed here, because it is one
palette across two windows.

### The ancestor-background assumption (AC 7) — real, and now derived

CPE-1921's *"no checked element paints its own background"* test read only the element's **own** rule
and only the **first** matching rule, so it could not see what actually decides the ground: the
ancestors. `#help-body h3` and `#keys-msg` sit on `Canvas` only because `#help-panel` / `#keys-panel`
paint it over their overlays' scrims.

Rewritten to parse the launcher's `<body>` into a real element tree (quote-aware tag scan; comments
and `<script>` / `<style>` bodies stripped first, per CPE-1933 rule 2) and walk **up** from each site
to the first ancestor that paints anything. **Red-proofed:** deleting `background: Canvas;` from
`#help-panel` alone fails with

> `#help-body h3: its ground is now painted by `#help-overlay` as `rgba(0,0,0,.45)`, not by `body {
> background: Canvas }`…`

which is exactly the silent ground-move the AC asked about — it would have taken `--accent-text` from
4.55:1 to roughly 1.35:1 in light with every assertion still green.

### Red-proofs (each run by hand; the guard named the culprit every time)

| sabotage | guard | result |
|---|---|---|
| focus border back to `var(--accent)` | harness | exit 1, **15 readings / 1 rule**: "2.46:1, below the 3:1 bar … state: select:focus" |
| `boot-pulse` trough back to `.45` | harness | exit 1: "**3.35:1** … state: animation frame 0%" |
| `.close-all-btn:hover` back to `#d05656` | harness | exit 1: "**3.65:1** on #f2f2f2" and "**4.13:1** on #1d1d1d" |
| drop `.pane-head`'s `color` | harness | exit 1: **1.09 / 1.10 / 1.10:1** on #161616, light |
| `.badge.yes` back to `#3a9d4a` | harness | exit 1: "3.44:1 … font-size 10px / weight 600" |
| `.tab` back to `GrayText` | harness | exit 1: **8 distinct rules**, 2.72 → 3.48 |
| `#help-panel` stops painting `Canvas` | vitest | fails naming `#help-overlay`'s `rgba(0,0,0,.45)` |
| focus rule back on `var(--accent)` | vitest | fails: "`var(--accent)` is the FILL token and is 2.46:1 against the dark `Field` interior" |
| a literal hex on a `:hover` rule | vitest | fails naming both `.close-all-btn:hover` declarations |
| a fixture's `derivedFrom` string removed | harness | refuses to run: "fixture provenance FAILED" |
| a wrong value in `validateAnchors` | harness | refuses to run before measuring anything (this fired for real on my 3.95 typo) |

### What the sweep now covers, and what it still cannot

**Covers.** `:hover` / `:focus` / `:focus-visible` / `:active` via CDP `CSS.forcePseudoState` — the
engine applying the state, not a rewritten stylesheet — one element at a time. Every CSS animation
paused and stepped through 21 frames, with the **worst** frame reported. `display:none` panels and
`[hidden]` overlays, deliberately **without** un-hiding them: computed colours resolve regardless of
`display`, and un-hiding stacks four `inset: 0` scrims over the page, which is how the first pixel
run read a predicted `#ffffff` ground as `#242424`. The JS-built DOM, via fixtures whose
`derivedFrom` strings are checked against the launcher's real builder source. Non-text roles
(`border-*-color`, `outline-color`, `box-shadow`, `background-color`) at 3:1, with a border measured
against **both** adjacencies. Literal hexes, which to a browser are just computed colours. Real
ancestor grounds, with opacity composited in paint order.

**Still cannot.** Colours the launcher's JS assigns **inline** (the session-chip palette — measured
and printed, not enforced; CPE-1977). The xterm terminal's own colours (vendor CSS injected at serve
time by `console.rs`, plus a `Terminal({ theme })` object). `::placeholder`, `::selection` and
scrollbar pseudo-elements. `forced-colors: active`, deliberately left alone since the UA replaces
every author colour there. And any ratio that depends on how a particular browser build resolves a
**system colour** — site 2 is the worked example, and the fix was to stop depending on one.

### Housekeeping

- `npm run check`: 0 errors, 0 warnings. `npm test`: **355 files / 5,146 passed**.
- `node scripts/ratchet-baselines.mjs compare origin/main`: all 12 baselines **unchanged**, none
  raised. (`launcher.html` is not a `.svelte` file, so the hex ratchet never saw it either way.)
- CI: a new `launcher-contrast` job in `gui-smoke.yml`, unconditional and blocking, pinned to Node 22
  for the global `WebSocket`, running with `--verify-pixels`.
  `src/lib/aiConsoleLauncher.contrast.test.ts` now asserts that job **and** its npm script still
  exist, so deleting the browser half cannot quietly turn the cheap half back into the whole story.
- In-app docs: `src/docs/35-appearance.md` gains a section on the Agent Deck following the OS scheme
  rather than the Theme control, what was fixed, and the one thing knowingly still out.
- **GUI verification would need a host rebuild, not a launcher swap.** `launcher.html` is
  `include_str!`'d into the ai-console host, so seeing these colours in the real app means rebuilding
  the sidecar-enabled host and installing that. Not done here.
