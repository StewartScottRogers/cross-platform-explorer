---
id: CPE-1985
title: the main app's chips are swept by **nothing** — the launcher got a real-browser contrast harness and the explorer never did
type: task
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Found by **CPE-1977**'s worker (PR #1102) while retuning the launcher's two inline palettes, and stated as
a declared gap rather than quietly left: **the main app's `.agent-chip` / `.menu-chip` on `--surface` /
`--bg` / `--hover`, across all four themes, are measured by no sweep at all.**

The values themselves **cannot drift** — CPE-1977 pinned `src/lib/sessionChip.ts` to `launcher.html`'s
array by parsing the launcher at run time and asserting equality including order, with a red-proof that
mutates the source. **But the grounds those chips sit on in the explorer are a different set of surfaces
from the launcher's tabs, and nobody has ever measured a chip against them.**

**The worker deliberately quotes no ratio for them anywhere in code**, on the grounds that a number with no
measurement behind it is a claim, not a fact. That restraint is why this is a ticket instead of a comment.

## Why the launcher has a harness and the app does not

`scripts/dev-harness/launcher-contrast/` (CPE-1966, extended by CPE-1977) mounts the **real** launcher
document in headless Chrome over CDP, reads `getComputedStyle` **and** cross-checks against a screenshot,
and derives every palette it tests **out of the source at run time** rather than from a copied list. It
exists because `launcher.html` is a standalone document with no module graph — the easiest surface in the
repo to mount whole.

The explorer is a Svelte app, so the equivalent needs a mounted component tree, which is why nobody built
it. **`scripts/dev-harness/organize-dialog/` (CPE-1968) is the proof that it is now cheap**: it mounts a
**real** Svelte component in headless Chrome at a fixed viewport and reads absolute layout out of it, and
`src/lib/svelteCss.ts` (same ticket) is the single derivation both dialog tests read component CSS through.
**The two halves of the instrument already exist and have never been put together.**

## What this needs

- [ ] **Sweep the app's chips against their real grounds**, in all four themes, in a real browser — not
      jsdom, which has no layout and no computed colour. Mount the real components; derive the palette from
      `src/lib/sessionChip.ts`, never a copy.
- [ ] **Get the bars right per role, because this is where CPE-1919 was learned.** A chip's **fill** is a
      UI shape at **3:1** (SC 1.4.11); a numeral **inside** it is text — and at the launcher's 10px/700 it
      is **normal** text at **4.5:1**, not large (large starts at 18.66px bold). CPE-1977 found seven of
      eight launcher colours missing one bar or the other precisely because those two bars leave one narrow
      luminance window. **Check what size and weight the app renders its chip numerals at before choosing a
      bar** — do not assume it matches the launcher.
- [ ] **Report a per-site verdict including the passes**, and **fail loudly on an empty population.**
      CPE-1977 added a `legsThatDidNotRun` floor to the launcher harness for exactly this: a sweep that
      measures nothing prints a clean report, and `STATE_META` sat in that state through all of CPE-1966.
- [ ] **Watch the dedup.** CPE-1977 found that all `.state-dot`s on a tab shared one key and the dedup kept
      the **worst**, so a neutral default won and was then dropped as non-chromatic — **it was not merely
      hiding the chromatic values from the report, it was removing them from enforcement.** Any sweep that
      dedups by selector-and-ground has this failure mode; split the key.
- [ ] **Do not quote a ratio you have not measured**, and if a ground cannot be measured, say so at the
      site rather than omitting it (the launcher harness's `--verify-pixels` currently reports 8 UNVERIFIED
      grounds per scheme against 59 verified — declared, not hidden).
- [ ] **Consider whether the guard belongs in CI.** `gui-smoke.yml` already runs a `launcher-contrast` job;
      an app-chip sweep is the same shape. If it is added, cap it with a measured `timeout-minutes` per
      CPE-1967 rather than a round number.

## A trap this ticket should not repeat

CPE-1977's first "retired hex" guard was **green 20/20** against the very colour it was named for, because
that hex was still a live value of a *different* token in the *dark* theme — **it structurally could not
fire for its own subject.** It was rewritten to difference the two resolved token maps. **A guard named
after a value must be tested against that value**, and the dead version is recorded at that site as the
worked example.

## Notes

Filed 2026-08-28 by the sprint Foreman from CPE-1977's worker (PR #1102), which declared the gap with its
reason instead of leaving it implied.

Related: **CPE-1977** (PR #1102 — the launcher's two palettes, the deleted `enforced()` exemption, and the
dedup trap), **CPE-1966** (the real-browser contrast harness), **CPE-1968** (PR #1099 — the Svelte
component harness and `svelteCss.ts`, which make this cheap), **CPE-490** (same colour and number in both
surfaces — the reason the app and the launcher share a palette at all), **CPE-1919** (`--accent` fills and
rings vs `--accent-text` reads — a token backing several roles gets pinned at the loosest bar, and that
assertion then reads like coverage), **CPE-1967** (measured job timeouts).
