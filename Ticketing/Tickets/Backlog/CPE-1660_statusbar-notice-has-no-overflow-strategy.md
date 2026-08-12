---
id: CPE-1660
title: A long status-bar notice wraps and grows the fixed-height bar — no overflow strategy at all
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-11
closed:
---

## Problem

Found by the independent UAT on PR #845 (CPE-1634), with a negative control.

The notice/toast text renders as a plain `<span>` in `src/lib/components/StatusBar.svelte:61-63`. That span
has **no `max-width`, no `white-space: nowrap`, and no ellipsis** — it has no overflow strategy at all. The
`.statusbar` itself declares a fixed `26px` height with no `overflow` rule.

So a notice longer than the window can hold simply **wraps to a second line**, and the bar visually grows
past its declared height. Nothing is clipped and nothing is lost — `#app`'s grid rows are `auto`-sized
(`src/app.css:754`), so the row just gets taller, and the notice auto-dismisses after 5 s — but the bar
stops being a fixed-height bar for those 5 seconds.

### Evidence (real headless Chrome, viewport verified from inside an iframe)

| Case | Width | Result |
|------|-------|--------|
| German `baselineIssues` (108 chars) | 900px | wraps to 2 lines, bar grows |
| **English** `baselineIssues` (99 chars) — negative control | 900px | fits on **1** line |
| German + English `archivePwProtectedExtracted` (186 / 172 chars, long filenames) | 1200px | **both** wrap |

The negative control is the important half: this is **pre-existing and language-independent** — English
reproduces it given a long enough string. It is not a defect introduced by CPE-1634. What CPE-1634 changes
is only how *often* it surfaces: German and Russian run ~10–20% longer than the English source, so some
notices that used to fit now tip over the wrap threshold at the same window width.

This is the same shape as the already-closed CPE-1635 finding for `CheckpointDialog`'s `h2`.

## Why it matters

A status bar that changes height for five seconds is a small jolt in an app whose stated tiebreaker is
*predictable*. It is also the last unconsidered case in a component that is otherwise fully specified: every
other piece of chrome in this app has an explicit overflow behaviour.

## Proposed fix

Give the notice span a real degradation path rather than letting it reflow the chrome. Options, in the order
they should be considered:

1. `max-width` + `white-space: nowrap` + `text-overflow: ellipsis`, with the full text in a `title`
   attribute so nothing is unrecoverable. Keeps the bar's height a constant.
2. Allow at most two lines (`-webkit-line-clamp: 2`) and give `.statusbar` a `min-height` instead of a fixed
   height, so growth is bounded and intentional rather than incidental.

Whichever is chosen, apply the same rule to the error variant (`noticeIsError`), and check the notice
alongside the other status-bar content at narrow widths — the bar is a flex row and the notice should not
be able to crush its neighbours either.

## Acceptance criteria

- [ ] A ~200-character notice at a 500px viewport leaves `.statusbar`'s rendered height **unchanged** from
      its height with a short notice (or at a documented, bounded maximum if option 2 is taken).
- [ ] The full text is still reachable by the user (tooltip/`title`) if it is visually truncated.
- [ ] The error-styled notice behaves identically to the normal one.
- [ ] The notice cannot squeeze the status bar's other contents out of view at a narrow width.
- [ ] A test pins it — render `StatusBar` with a long notice in a **verified** viewport and assert the
      height. Note the harness rule: headless Chrome's `--window-size` does NOT set the CSS viewport under
      `--headless=new` (it clamps to ~500px and rescales), so mount in an iframe and confirm the width from
      inside it. A prior false defect report came from exactly this.
- [ ] Deliberately remove the new CSS rule and watch the test go red, so the guard is pinned rather than
      vacuous.

## Notes

Filed by the Foreman from the PR #845 UAT report, 2026-08-11. The UAT explicitly graded it **cosmetic and
non-blocking**, and PR #845 merged on that basis; this ticket carries the residue.
