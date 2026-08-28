---
id: CPE-1966
title: four more AI Console launcher contrast defects — including the **only focus indicator** at 2.46:1 — that CPE-1921's new guard structurally cannot see
type: bug
priority: Medium
status: Open
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
