---
id: CPE-1095
title: "Code preview polish: fold-aware jump-to-symbol + doc wording"
type: bug
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-26
epic: CPE-724
---

## Summary
Two minor, non-blocking items the CPE-1091 reviewer flagged (both explicitly "author's discretion — no fix
required to merge"), captured here so they aren't lost:

1. **Fold-aware jump-to-symbol.** `jumpToSymbol` in `src/lib/components/PreviewPane.svelte` computes
   `(line-1)*lineHeight` and does NOT subtract lines currently hidden by a collapsed fold *above* the target,
   so a jump can land a few rows off while a fold above it is collapsed. Same uniform-line-height assumption
   that already degrades under the wrap toggle (pre-existing from CPE-1090); uncommon interaction, not a
   regression — but worth fixing. Fix: when computing the scroll target, subtract the count of hidden lines
   whose line number is `< sym.line` (walk the collapsed `FoldRange`s / `hiddenLines` set), OR scroll to the
   target row's actual `getBoundingClientRect` when it's rendered (more robust — the row for `sym.line` is a
   `.cl-row[data-line=N]`, so `container.querySelector('[data-line="N"]')?.getBoundingClientRect()` gives the
   true position regardless of folds/wrap; fall back to the line-height math when the row is itself hidden
   inside a collapsed fold, expanding that fold first). Do the same offset correction for `updateBreadcrumb`.

2. **Doc wording.** `src/docs/03-explorer.md` frames the new gutter/indent guides as for "source-code files,"
   but every `text`-provider file (including plain `.txt`) now gets the line-number gutter. Reword to reflect
   that line numbers/indent guides apply to text files generally (line numbers on plain text are reasonable).

## Acceptance Criteria
- [ ] Jumping to a symbol lands on that symbol's line even when a fold above it is collapsed (row-rect based,
      or hidden-line-count corrected); breadcrumb reads the true top-visible line under the same condition.
- [ ] `03-explorer.md` wording matches actual behaviour (text files, not just source code).
- [ ] `npm run check` clean; vitest green (add a unit test for the hidden-line offset helper if one is
      extracted); no new deps.

## Work Log
2026-07-26 (workshift, GUI) — Filed by the Foreman from the CPE-1091 reviewer's two non-blocking notes so the
polish isn't lost. Low priority; pickable anytime after GUI #1 shipped.
