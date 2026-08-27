---
id: CPE-1919
title: dark-theme JSON string values measure 3.70:1 contrast, below the 4.5:1 bar for 12px text
type: bug
priority: Medium
status: Open
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
