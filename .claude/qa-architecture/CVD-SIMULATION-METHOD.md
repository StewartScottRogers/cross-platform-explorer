# Colour-blind (CVD) simulation method — the crew's standard

**Decision (CPE-1648):** when a ticket needs to judge whether two UI colours are distinguishable
under colour-vision deficiency, use the **Machado, Oliveira & Fialho (2009)** dichromacy matrices
— the same family **Chrome DevTools' Rendering → "Emulate vision deficiencies"** panel uses. This
is now the crew's one standard; don't re-derive or re-argue it per ticket.

## Why this method, and not Brettel/Viénot or another family

CPE-1648 was filed because three independent assessments of the CPE-1631 syntax-highlighting
palette (`--hljs-*` in `src/app.css`) used **different published dichromacy matrix families** and
got materially different worst-case numbers for the same colour pairs — nobody was wrong, the
methods genuinely disagree on magnitude for near-collision pairs (see the ticket's "The
disagreement, on the record" section for the specific before/after numbers each method produced).
That meant the crew had no way to answer "is this palette safe?" with confidence, and every future
theming ticket would hit the same wall.

Machado 2009 wins on **practical reproducibility**, not on being more "correct" than Viénot/Brettel
1999 (both are legitimate, peer-reviewed dichromacy models — they just weight the LMS→dichromat
projection differently):

- **It's what a developer can actually toggle and look at.** Chrome DevTools' CVD emulation is one
  click away for anyone on the crew (or the user) — no separate tool, no bespoke script to trust.
  Reproducing a finding is "open DevTools, toggle protanopia, look at the app," not "re-run someone's
  one-off matrix script and hope the coefficients match."
- **It's already the family this repo cites.** `src/app.css.hljs-contrast.test.ts`'s own docstring
  already names "Machado, Oliveira & Fialho 2009 dichromacy matrices, the same family Chrome
  DevTools' 'Emulate vision deficiencies' uses" — CPE-1648 makes that the *decided* standard instead
  of an incidental citation.
- **It matches the tool people will reach for.** When a future ticket asks "does this new palette
  read OK for colour-blind users," the fastest real check is: open the app in Chrome/Edge DevTools
  (or the wry/WebView2-hosted app via its DevTools), Rendering panel → Emulate vision deficiencies →
  protanopia/deuteranopia, and look at real rendered content. Standardising on the matrix family that
  panel uses means a from-the-hip DevTools check and a scripted/CI check never disagree in kind, only
  possibly in exact pixel values (which don't matter for a pass/fail judgement call).

## How to apply it

**Preferred — live, in the real app:** Chrome/Edge DevTools → `⋮` menu → More tools → Rendering →
"Emulate vision deficiencies" → pick Protanopia or Deuteranopia. Look at real rendered content (a
real file preview, a real dialog), not swatches in isolation — collisions that matter are the ones a
user actually hits.

**Scripted/headless check (no live DevTools session available):** apply the same Machado
full-severity matrices as an SVG `feColorMatrix` filter (`color-interpolation-filters="sRGB"`) over
the rendered surface, screenshot, and inspect. Matrices (protanopia / deuteranopia; the same
coefficients power both the live DevTools panel and most CVD-simulator libraries built on this
family):

```
protanopia:    0.152286  1.052583 -0.204868
               0.114503  0.786281  0.099216
              -0.003882 -0.048116  1.051998

deuteranopia:  0.367322  0.860646 -0.227968
               0.280085  0.672501  0.047413
              -0.011820  0.042940  0.968881
```

(3x3, applied to sRGB `[r,g,b]` in `0..1`, alpha untouched — see any CPE-1648 work-log entry for a
worked SVG `<filter>` example.)

## CPE-1648's own finding, using this method

Ran this check against the real CPE-1631/CPE-1543 `--hljs-*` palette (all four themes: light, dark,
hc-light, hc-dark) on real Rust code containing all six highlighted buckets (keyword, title, string,
comment, number, tag), under both protanopia and deuteranopia emulation. **No pair was genuinely hard
to tell apart** — every pair stayed clearly separated by hue and/or the existing non-hue cues
(keyword/title are bold, comment is italic). The closest pair by eye was `string` vs `tag` in
hc-dark (both read as pale green under protanopia/deuteranopia) — visually distinguishable but the
smallest margin found; not a collision, and no fix was made because none was warranted (re-hueing a
non-colliding pair would just be matrix-chasing, the exact anti-pattern this ticket exists to end).
See CPE-1648's ticket work log for the full screenshot set and the redmean colour-distance table
computed under both matrices, across all four themes and all six-choose-two token pairs.

WCAG luminance contrast is a **separate, unaffected concern** — `app.css.hljs-contrast.test.ts`
already asserts every `--hljs-*` token clears its theme's WCAG bar against `--surface`/
`--surface-alt`. CVD simulation only answers "can two different hues be told apart," not "is there
enough light/dark contrast" — keep using both checks, they don't substitute for each other.
