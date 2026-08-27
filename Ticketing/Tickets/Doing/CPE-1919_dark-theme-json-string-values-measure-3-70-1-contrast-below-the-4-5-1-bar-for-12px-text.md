---
id: CPE-1919
title: dark-theme JSON string values measure 3.70:1 contrast, below the 4.5:1 bar for 12px text
type: bug
priority: Medium
status: In Progress
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
