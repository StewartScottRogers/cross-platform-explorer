---
id: CPE-1965
title: `organize.smoke.ts` fails on gui-smoke shard 4 with `element not interactable` — a NEW case, and **not** CPE-1960's shape
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

    NEW GUI REGRESSION: "organize.smoke.ts :: opens via the command palette, picks a rule,
      and renders grouped proposal rows"
    ERROR webdriver: WebDriverError: element not interactable when running
      "element/node-9D570771-.../click" with method "POST"

Observed on **PR #1074** (job at 2026-08-28T01:38Z): shard 4 reported `14/14 spec file(s) reported,
29 case(s) — 25 passed, 4 failed, 0 skipped/pending`, `incomplete=false`, **3 known-failing listed**,
so this is the **fourth** failure and the only unlisted one.

**Measured against `main`:** run `33131388244` (01:31Z, `main`) reported shard 4 as
`26 passed, 3 failed, 3 known-failing` — i.e. clean, every failure accounted for. Seven minutes later
the same shard on #1074 carries one extra.

## Why this needs its own ticket rather than a re-run

**It is not CPE-1960.** That one is `element (".ctx .flyout .row") still not existing after 5000ms` —
an element that never appears, caused by webdriverio 9.31.4's `scrollIntoView` injecting a real mouse
wheel at viewport (0,0) and closing a flyout on `mouseleave`. This one is **`element not
interactable` on a `click`** against an element the driver *found*. Different failure, different
mechanism, and it must not be filed under CPE-1960's diagnosis by proximity.

**#1074's diff cannot plausibly cause it.** That PR touches `.github/workflows/ci.yml`,
`scripts/ci-verdict.mjs`, and two vitest files. Nothing it changes reaches a gui-smoke spec. So this is
either a genuine intermittent that happened to land on #1074, or something in the shared harness.

**The re-run reflex is exactly what CPE-1955/CPE-1960 cost a day to.** Runs that failed illegibly got
re-run; runs that failed legibly got re-run too; a real, named regression was discarded for a day
because nobody wrote it down. **Do not re-run and move on.** One clean `main` run and one red #1074 run
is two data points, not a diagnosis.

## Worth ruling in or out first

- **A second wdio 9.31.4 casualty.** CPE-1960's root cause was a lockfile-only bump (9.30.0 → 9.31.4)
  that turned `element.scrollIntoView()` from a no-op into a real wheel event. **PR #1072 replaced
  seven command call sites but `organize.smoke.ts` was not among them** — and the failing job's log
  shows `document.querySelector(".recent-row")?.scrollIntoView({block:"center"})`, i.e. a **DOM-API**
  call inside `browser.execute`, which is the *correct* form and should be inert. Establish whether any
  wheel is dispatched near the failure. If a scroll moved the target between find and click, `element
  not interactable` is exactly what you would see, and this is the same family with a different
  symptom.
- **A genuine app defect** in the command palette → rule-pick → proposal-rows path.
- **A spec that clicks before the element is ready** — a real defect, but a different one.

## Acceptance criteria

- [ ] **Establish a rate, not an observation.** Enumerate every shard-4 job in the window and
      fingerprint what `npm ci` installed (`added 479 packages` = wdio 9.30.0, `489` = 9.31.4) — that
      is what settled CPE-1960 after three wrong characterisations, and it is cheap. **The discriminator
      is what the runner installed, not what the branch merged.**
- [ ] Say plainly whether it is **the app**, **the spec**, or **the harness**, and give the evidence.
- [ ] **Do not add it to `known-failing.json` as the fix.** If it genuinely must be deferred, the entry
      needs a ticket and a reason, and the deferral must be argued.
- [ ] **Check `organize.smoke.ts`'s neighbours on shard 4** for the same click-before-ready pattern —
      `batch-media`, `context-menu`, `declutter`, `home-item-menu`, `macro-in-menu`, `metadata-studio`,
      `preview-pane`, `saved-search`, `snapshot-diff`, `terminal-panel`, `trash-titlebar`, `vault`.
      Enumerate rather than recall (CPE-1932).
- [ ] While there: **identify the 3 known-failing shard-4 cases** and confirm each still has a live
      owning ticket and reason. One of them surfaces in the same log as
      `Error: expected the permanent Network section header to render` (`network.smoke.ts`).
- [ ] Red-proof: show the failing condition and show it gone, at a rate comparable to the reproduction.

## Notes

Filed 2026-08-27 by the sprint Foreman, on finding it while verifying #1074's reds. **It blocks #1074's
merge** until it is understood — that PR is otherwise APPROVED with a clean review.

Related: **CPE-1960** (the *other* shard failure — different shape, do not conflate), **CPE-1955** (the
attribution fix that made shard failures legible at all), **CPE-1910** (shard 2's WebDriver socket
deaths), **CPE-1171** (the gui-smoke harness), **CPE-1728** (the slow-renderer family).
