---
id: CPE-1919
title: dark-theme JSON string values measure 3.70:1 contrast, below the 4.5:1 bar for 12px text
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

In the JSON tree preview, **string values in the dark theme** render blue `#0078e0` on background
`#202020`, which measures **3.70:1**. WCAG AA requires **4.5:1** for text at this size (12px). The
light-theme equivalent is fine at 5.11:1.

Measured 2026-08-27 by the independent Visual Critic on PR #1038, from
`.claude/sprint-metrics/visual-evidence/cpe-1876-dark-json.png` — the affected values in that
screenshot are `"cross-platform-explorer"` and `"0.57.67"`.

## This is a colour-token defect, not a CPE-1876 regression

PR #1038 changed only `font-family`. The same blue was there before it and is there after. The
Critic explicitly cleared #1038 of this and recommended it be filed on its own, in the
**CPE-1810 / CPE-1821 token family**.

## Contrast measured across the same surfaces, for context

Every other reading in the same pass was comfortably above bar — this is a single outlier, not a
systemic palette problem:

| Surface | Light | Dark |
|---|---|---|
| binary Address cell (11.5px mono) | 15.5:1 | 14.7:1 |
| binary footer note | 4.73:1 | 5.93:1 |
| log line body (12px mono) | 16.7:1 | 13.6:1 |
| log DEBUG / TRACE tags | — | 8.2:1 / 5.5:1 |
| diff context / removed / added | 17.2 / 12.5 / 13.5:1 | 12.8 / 9.9 / 9.6:1 |
| json key | — | 9.4:1 |
| **json string value** | 5.11:1 | **3.70:1** |

## Acceptance criteria

- [ ] Lift the dark-theme JSON string-value colour to **at least 4.5:1** against its actual
      background, without breaking its distinguishability from the key colour (9.4:1) or from the
      number/boolean/null colours beside it — a fix that makes strings legible but indistinguishable
      from keys trades one defect for another.
- [ ] Change the **token**, in both the light and dark blocks, not a component-local hex. Semantic
      tokens only; never a hard-coded colour.
- [ ] Check whether the same token is used elsewhere and whether those sites are also below bar —
      fix the token's every consumer, not just the JSON tree.
- [ ] **The existing WCAG guard test did not catch this.** Establish why (is this pairing simply not
      enumerated? is the guard checking a nominal background rather than the painted one?) and extend
      it so this pairing is pinned. A contrast guard that misses a 3.70:1 body-text pairing is the
      "guard that proves nothing" pattern this repo keeps re-finding — that half matters more than
      the colour change.
- [ ] Re-measure from a real screenshot after the fix, not from the token values alone.

## Notes

Filed 2026-08-27 by the sprint Foreman from the Visual Critic's measured findings on PR #1038.

## Work Log

### 2026-08-27 — fixed + pinned (branch `cpe-1919-accent-text-contrast`)

**Root cause is a token with three roles and one value.** `--accent` backs (a) the solid-fill
background of every `.btn.primary`-style button under white text, (b) icon glyphs / focus rings /
borders, and (c) running text. (a) and (b) answer to WCAG 1.4.11's 3:1; (c) answers to 1.4.3's
4.5:1. CPE-1632 tuned the dark value for (a)+(b) — `#0078e0`, white-on-fill 4.41:1 — and nobody
measured (c). `JsonTreeNode.svelte` paints role (c) at 12px.

**The measurement was also against the wrong ground.** The ticket's 3.70:1 is `--accent` on `--bg`.
The JSON tree does not sit on `--bg`: `.preview-pane` paints `background: var(--surface)`, so the
real reading is **3.21:1**, and `.jt-row:hover` repaints the row `--surface-alt` (3.43:1) — a third
ground no palette guard measures text against at all. All three confirmed from screenshot pixels.

**Fix.** New semantic token `--accent-text`, defined in all five live theme selectors (bare `:root`,
light, dark, hc-light, hc-dark):

| theme | `--accent-text` | note |
|---|---|---|
| light / bare `:root` | `var(--pal-blue-600)` `#0067c0` | same value as `--accent` — light already cleared 4.5:1; split out anyway so a future `--accent` re-tune can't silently re-break text |
| dark | new `--pal-dark-blue-350` `#3aa0f0` | same hue (~207 deg) as `#0078e0`, lightened; the darkest value on that hue line that clears 4.5:1 on all three grounds, so it stays maximally separated from `--text` |
| hc-light | `var(--pal-hc-light-blue-900)` `#0043ce` | already past the hc AAA text bar |
| hc-dark | new `--pal-hc-dark-blue-250` `#72abdf` | hc-dark's `--accent` was **4.48:1 on `--surface-alt`** — a second, unreported failure this ticket's sweep found |

`--accent` itself is unchanged, so every button, ring and border is untouched.

**Every consumer, not just the JSON tree (AC 3).** Enumerated all 34 `color: var(--accent)` sites.
22 are running text and now use `--accent-text`: the JSON string value, markdown/notebook/card-body
link text, `.note` in Checkpoint/Integrity/Macros/Templates, `.status` in BackupDashboard/Conflict/
RepoBrowser, the Batch-Rename and Batch-Media "to" filename, `.cd-id`, `.cmd.on`, `.log-badge`
(INFO), `.op-kind`, HomeView `.clear`, HotkeyCaptureInput `.capture.armed`, StatusBar
`.filtered-hidden`, AboutDialog `.link`. The 12 left on `--accent` are icon glyphs and checkmarks
(`.ic`, `.menu .check`, `.iconbtn.on`, `.pin.pinned`, `.vb-icon`, VaultBadge) — genuinely non-text
UI at the 3:1 bar the existing guards already pin.

**Guard (AC 4) — `src/app.css.accent-text-contrast.test.ts`.** Establishes *why* the old guard was
green: the pairing was enumerated at the **wrong bar**, not missing. `dark-contrast.test.ts` asserts
`--accent` vs `--bg`/`--surface` at >=3:1 and labels it "text/icon/focus-ring accent" — a token
backing several roles is always pinned at the loosest of them, and that assertion reads like
coverage. The new guard: (1) `--accent-text` resolves in all five theme selectors; (2) it clears the
text bar on every painted surface in every theme; (3) **every** colour role in the JSON preview,
derived by parsing `JsonTree.svelte` + `JsonTreeNode.svelte` at run time rather than a hand-kept
list (CPE-1932), clears the bar on every surface in every theme; (4) the painted surfaces are
themselves derived — `.preview-pane`'s `background` and `.jt-row:hover`'s fill are read out of the
real CSS and the read throws if either stops setting one, so the guard can never grade against a
ground nobody paints; (5) `--accent-text` is never used as a `background`; (6) the string colour
stays a different resolved hex from the key / number / null colours in every theme.

**Red-proof.** Two, both run:
- Set `--pal-dark-blue-350` back to `#0078e0`: 2 tests fail, naming the token, the surface and the
  ratio — `dark: --accent-text (#0078e0) on --surface (#2b2b2b, .preview-pane background) = 3.21:1,
  want >=4.5:1` (and 3.70:1 on `--bg`, reproducing the ticket's own number).
- Put `.jt-val.jt-string` back on `var(--accent)`: fails with "the JSON tree must paint string values
  with --accent-text", plus six ratio failures — including hc-dark's pre-existing 4.48:1.

**Re-measured from real screenshots (AC 5), not from token values.** Headless Chrome renders the
real `JsonTree.svelte` in the real `.preview-pane` chrome with the real `src/app.css`; the capture is
drawn back onto a canvas and each role's glyph core is sampled against its own box's modal pixel.
Dark string value: **3.22:1 before -> 5.03:1 after**, on a rendered ground of `#2b2b2b` (confirming
the pane paints `--surface`, not `--bg`). Light unchanged at 5.67:1. Evidence:
`.claude/sprint-metrics/visual-evidence/cpe-1919-{light,dark}-json-preview{,-before}.png`.

**No ratchet moved** — `node scripts/ratchet-baselines.mjs compare origin/main` reports all 12
baselines unchanged (hex ratchet still 85 files / 277 occurrences); this fix adds no hex literals
outside the palette layer.

Docs: `src/docs/35-appearance.md` gains an "Accent-coloured text stays readable" section (no new
`Section`, so `sectionDocs.ts` is untouched); `CLAUDE.md` gains the `--accent` vs `--accent-text`
convention under UI conventions.

`npm run check` clean; `npm test` 346 files / 4932 passing.

### 2026-08-27 — review round on #1069: seven more text sites, and the sweep that finds them

The Visual Critic returned three findings. All three were real, and chasing them found four more of
the same class.

**1. `RepoBrowser.svelte` `.repo-crumb`** — left on `--accent` four lines below `.repo-status.ok`,
which had been migrated. Both 12px body text, ~20px apart in the same panel, separated by a
hairline: the panel rendered **two different blues**, with the clickable breadcrumb path the duller
one at 3.21:1. Fixed.

**2. `UserCommandsDialog.svelte` `.pill.surf`** — 10px pill label on `--surface-alt` at **3.43:1**:
smaller text at worse contrast than the JSON case this ticket was filed for. `color` moved; the
border correctly stays `--accent`.

**3. Stale provenance in `StatusBar.svelte`.** Moving `.filtered-hidden` to `--accent-text` made the
CPE-1883 block ~110 lines below false — it still asserted "the pill underneath KEEPS `--accent`" and
quoted 3.21:1 as a live condition, with a green test beside it. Rewritten to record what CPE-1883
measured, what CPE-1919 changed, and why `color: var(--text)` stays on the reveal for its **own**
reason (the low-vision affordance earns the highest-contrast tone, 12.76:1) rather than as a
workaround for a hazard that no longer exists. Two neighbouring comments naming `--accent` were
corrected the same way.

**Four more, found only by widening the spelling.** The original sweep grepped bare
`color: var(--accent)`. Five sites use `var(--accent, <fallback>)` and were invisible to it — and
one used `--accent-hover`:

| site | text | before |
|---|---|---|
| `AgentTimeline .tl-expand` | "Open full diff ⤢" button, 10.5px on `--surface-alt` | 3.43:1 |
| `AgentTimeline .rp-play` | "Play"/"Pause" — a word, not a glyph | 3.21:1 |
| `AgentTimeline .rp-speed-btn.active` | "1×" / "Cost" / "Tokens" | 3.21:1 |
| `IcalPreview .cp-badge` | 10px uppercase badge label | 3.21:1 |
| `SidecarManager .logs-toggle.repair` | "Repair" button label | 3.21:1 |
| `AboutDialog .link:hover` | link hover used `--accent-hover` | 4.10:1 |

The first five moved to `--accent-text`. `.link:hover` went to `--text` instead of inventing an
`--accent-text-hover` for its single call site: brightening to the full text tone is a clearer hover
affordance than a half-step, and it passes trivially in every theme.

**The sweep (the coordinator's suggestion, and the real answer).** The per-surface guard cannot see
any of these — it only measures surfaces someone thought to point it at, which is the same blind
spot as measuring a token at the loosest of its bars. So the guard now inverts the default: it finds
**every** `color:` in `src/` resolving to `--accent`/`--accent-hover`, in both spellings, and fails
on each unless its selector is declared in `ICON_ROLES` with a note saying which glyph it paints.
An allowlist rather than a heuristic, because nothing in CSS distinguishes a checkmark from a word,
and a guard that guesses "icon" is no guard. A third test fails on any `ICON_ROLES` row that stops
matching anything, so an exemption can't outlive the thing it excuses.

The 11 surviving `--accent` colour sites are all genuine glyphs (`.iconbtn.on`, `.menu .check`,
`ContextMenu .check`, `MenuBar .mb-check`/`.check`, `.pin.pinned`, three `.ic` icon cells,
`VaultBadge`, `VaultBanner`) — exactly the set the Critic independently arrived at.

**Red-proofs, all run.** Reverting `.repo-crumb` and `.pill.surf` fails naming both by file,
selector and remedy. Adding a bogus `ICON_ROLES` row fails naming the stale row.

Also confirmed by the Critic and worth keeping on the record: light is **byte-identical** before and
after (same md5, not merely "unchanged"), the dark string glyphs are the only pixels that move, row
hover *raises* contrast to ~5.38:1 (`--surface-alt` is darker than `--surface`), and the
`--accent-text`-inside-an-`--accent`-border pairing at `CardDetailDialog .cd-id` reads as an ordinary
outline chip.

Ratchets still all 12 unchanged. `npm run check` clean; `npm test` 346 files / 4940 passing.

### 2026-08-27 — review round 2 on #1069: I shipped a wrong contrast ratio into CLAUDE.md

**The blocker was mine and it was the ticket's own defect, one level up.** I wrote "white on
`--accent-text` is 3.53:1 (dark) / 1.90:1 (hc-dark)" in four places — `src/app.css` twice, the guard
test's header, and **`CLAUDE.md`**, the repo's primary convention doc. Both numbers were estimated,
not measured. Run through the very `contrastRatio` the guard uses:

| pair | I wrote | actual |
|---|---|---|
| `#ffffff` on `#3aa0f0` (dark `--accent-text`) | 3.53:1 | **2.81:1** |
| `#ffffff` on `#72abdf` (hc-dark) | 1.90:1 | **2.44:1** |
| `#ffffff` on `#0078e0` — control | — | **4.41:1**, matching CPE-1632's independent record |

The conclusion holds *harder* — both true values are further under the 3:1 floor — but as written the
dark sentence **contradicted itself**: 3.53 is above the floor I cited it as being below. A
maintainer reading CLAUDE.md would have got a number saying the opposite of the sentence around it.
In a PR whose thesis is "a ratio recorded at the wrong bar reads like coverage", that is the same
defect in the prose. All four corrected, each site now carrying why the distinction earned a
sentence: measure, don't recall.

**The guard claimed a mechanism it did not have.** The header said assertion (a) made "a token
missing from one theme loud rather than silently inherited". The reviewer deleted `--accent-text`
from the `hc-dark` block: `hc-contrast`, `app.css.test`, `warn-token` **and (a) itself** all stayed
green — `tokenHex` resolved the bare `:root` value through the palette map and got a valid hex. Only
the ratio tests caught it, because the inherited light blue then measured 3.70/3.43/3.07 in hc-dark.
Safe by luck: had the inherited value cleared the bar, the omission would have shipped.

Rather than soften the claim I added the mechanism — a second assertion that each theme block
**declares** the token itself, not merely resolves it. Red-proofed by repeating the reviewer's
deletion: it now fails with `--accent-text missing from the block(s): hc-dark`. Recorded at the site,
because it is worth knowing generally: **this repo has no general theme-parity guard.**
`app.css.test.ts` checks bare `:root` vs light only; the dark and hc guards each check their own
theme against a hand-kept fixture a new token never gets added to.

**Two more, both taken.** `src/docs/35-appearance.md` said string values "used to sit at 3.7:1 —
visibly dim against the preview pane": 3.7 is the `--bg` reading, so that sentence re-committed the
exact `--bg`-vs-`--surface` conflation this PR corrects. Now **3.2:1 against the pane** — truer and
more damning. And `IcalPreview .cp-badge` painted `background: var(--accent-soft, var(--surface))`
where **`--accent-soft` is defined nowhere in the repo**. Harmless today, and the fallback is what
makes that badge's pinned 5.03:1 correct — but a never-populated first choice sitting in front of a
ground that feeds a pinned ratio is a trapdoor: define `--accent-soft` later and the ratio silently
goes wrong with nothing to catch it. Now `background: var(--surface)`, with `warn-token.test.ts`'s
"current instances" comment updated so it does not name an instance that no longer exists.

**Count settled at eight** (two reported + five behind the `var(--accent, <fallback>)` spelling +
`AboutDialog .link:hover` on `--accent-hover`), broken out in the guard comment rather than left as a
bare total, since two different sevens were circulating.

**One latent hole recorded, not fixed:** the sweep's `(?<![-\w])color` lookbehind also rejects
`-webkit-text-fill-color`, `text-decoration-color` and `caret-color`, which *are* text roles. None
exists anywhere in `src/`, so it is a hole in the net rather than a fish through it — but a
fail-closed allowlist exists to catch the next person, so the comment tells them to widen the match.

Ratchets still all 12 unchanged. `npm run check` clean; `npm test` 346 files / 4947 passing.

### Round 4 — the parity-gap account in my own note was false, and I had not measured it

The reviewer took the sentence I was proudest of and disproved it. Round 3's note said flatly
**"THIS REPO HAS NO GENERAL THEME-PARITY GUARD"** — written from reading two `SEMANTIC_TOKENS.filter`
calls and generalising. It is wrong for the `dark` block. `src/app.css.dark-contrast.test.ts` carries
a **second, fixture-independent** check (`lightOnly`, ~lines 155-161) whose own comment says it
"keeps the fixture itself honest if a future ticket adds a new semantic token to light but not
dark" — a brand-new token joins it automatically, no fixture edit needed.

Measured both halves before rewriting, since replacing an unmeasured claim with another unmeasured
claim would have been round 2's defect a third time:

- **`--accent-text` deleted from the `dark` block** → `dark-contrast.test.ts` **RED**, 1 failed / 12
  passed, at `dark-contrast.test.ts:160` with `tokens present in light but missing from dark:
  --accent-text` — **naming the new token**, which appears in no fixture. Covered.
- **The same deletion from `hc-dark`** → `hc-contrast.test.ts` **GREEN**, 23/23. That file has only
  the two `SEMANTIC_TOKENS.filter(...)` checks and no symmetric counterpart. Not covered.

So the true shape is uneven, not absent: `dark` is guarded for new tokens; bare `:root`/`light`
(`app.css.test.ts`) and `hc-light`/`hc-dark` (`hc-contrast.test.ts`) are not. The note now carries
that as a three-row table with both measurements written beside it, and points at **CPE-1962** to
give the two unguarded pairs the symmetric check `dark` already has.

This mattered more than a wording nit. The note is the repo's written account of the gap and a
follow-up ticket is being filed from it; as written it aimed that ticket at a half-solved problem and
told the next maintainer that light↔dark parity is unguarded when it is guarded — the exact opposite
of the truth, **inside a comment added specifically to stop people trusting unchecked claims.** The
lesson is the one this ticket keeps re-teaching in a new costume: a negative claim ("nothing checks
X") needs a measurement just as much as a positive one, and is harder to notice going unmeasured.

**Second, non-blocking: the strict half has a hole the loose half covers.** The reviewer attacked the
new DECLARED assertion six ways; five held (deletion, comment-out, mis-casing, whitespace/multi-line,
selector-list — the last over-strict but failing closed). The sixth: `--accent-text: ;` **passes**
it. Verified the mechanism in isolation before writing it down — `(--[a-zA-Z0-9-]+)\s*:\s*([^;]+);`
backtracks, the `\s*` after the colon yielding its space so `[^;]+` can consume it, so the token is
recorded with an empty value and `decls.has()` returns true. Verified end-to-end too: writing
`--accent-text: ;` into the hc-dark block leaves DECLARED **green** and turns **three** other tests
red (`hc-dark -> (unresolved)` plus both ratio tests). Defence-in-depth holds, so this is documented
at the site rather than patched — tightening `extractDecls` risks a false positive on the valid
multi-line declaration the reviewer confirmed is accepted today, and the regex is shared with the
palette-resolution path. A check advertising itself as strict while the loose one covers its gap is
worth a sentence, not a rewrite.

Gates unchanged: `npm run check` 0 errors; `npm test` 346 files / 4947 passing, 2 skipped, 0
failures; ratchets all 12 unchanged; hex 85/277. Rebased on `origin/main` (809f9c7c) first.

## Closed 2026-08-27 — what the gauntlet actually proved

Merged as PR #1069, after **four rounds**. Both blockers were the same defect wearing different
clothes: **a claim written as measured that had not been measured.**

**Round 2 — the ratios were estimated, not measured.** The PR stated `3.53:1` and `1.90:1`. The real
values are **2.81** and **2.44**. Corrected in four places including `CLAUDE.md`. The author's response
was the right one: rather than soften the guard's overstated claim, it gave the guard the *mechanism* —
the file now asserts each theme block **declares** the token, not merely that the token resolves.

**Round 3 — a claim about other files' coverage was false, in the safe-sounding direction.** The new
comment said *"there is no general theme-parity guard in this repo"*, written specifically so nobody
would trust an unchecked assumption. Its Reviewer disproved it **by deletion**: removing
`--accent-text` from the `dark` block reds `dark-contrast.test.ts` **by name**, because that file
carries a fixture-independent symmetric check a brand-new token joins automatically. The same deletion
from `hc-dark` leaves `hc-contrast.test.ts` green. So `dark` is guarded; bare `:root`/`light` and both
`hc-*` blocks are not. Understating existing coverage would have aimed the follow-up ticket at the half
already solved. Filed properly as **CPE-1962**.

**A negative claim about coverage is a claim about several other files at once** — and it is checkable
by deletion, not by reading. That is the transferable lesson, and it is the same family as CPE-1933.

**The guard survived five of six attacks on its parse claim** (deletion, comment-out, mis-casing,
multi-line whitespace, selector-list — the last over-strict but failing **closed**). One narrow hole
documented rather than patched: `--accent-text: ;` passes the DECLARED assertion through regex
backtracking, and is caught by the resolve half. The regex was deliberately **not** tightened, because
`extractDecls` is shared with the palette-resolution path and tightening risks a false positive on the
valid multi-line form.

**Also settled:** `--accent-hover` on `--surface` in dark measured **4.10:1**, *below* the new rest
state's **5.03:1** — the link was **dimming** on hover, not merely half-stepping. The site count is
**eight**. `--accent-soft` was a dangling reference to an undefined token sitting in front of a ground
that feeds a pinned ratio; removed, with `warn-token.test.ts`'s stale "current instances" comment
corrected. A latent hole is recorded at the site: `-webkit-text-fill-color`, `text-decoration-color`
and `caret-color` are all real text roles the sweep's lookbehind rejects; none exists in `src/` today.

**Merged past a known red**: shard 2 and its verdict job were failing on CPE-1960, verified by reading
the job log rather than trusting the job name.
