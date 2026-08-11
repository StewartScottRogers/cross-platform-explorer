---
id: CPE-1629
title: "gui-smoke has no spec for the preview pane, so every new preview surface needs a hand-built Chrome harness to be seen at all"
type: Task
status: Backlog
priority: Medium
component: Testing
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Manual-verification debt, surfaced by the Visual Critic reviewing CPE-1615 (PR #820). The Binary Inspector
gained a whole new tab, and the `gui-smoke` harness had **no spec covering that surface** — so the CI run
produced no screenshot of it, and the only way to actually look at the change was to hand-build a
throwaway Vite + real-Chrome harness that mounted the component against canned data.

That harness worked (and correctly found the tab strip, pill reflow, and theme fidelity all sound), but
it was rebuilt from scratch for one review and thrown away. Every future preview-pane change pays that
cost again — or, worse, ships unlooked-at. `gui-smoke` exists precisely so nobody has to do this by hand.

This matters more than it looks: this crew's hardest-won lesson is that **jsdom cannot see layout** — 3,231
tests once passed while every submenu in the app was clipped invisible. A surface with no screenshot spec
is a surface where the test suite's green is silent about how it looks.

## Goal
Give the preview pane first-class screenshot coverage in `gui-smoke`, so a change to any preview provider
is automatically captured and can be judged from CI artifacts alone.

## Scope
- Add `gui-smoke` specs that open the preview pane against **committed sample files** (the `samples/`
  tree already exists and is ratcheted by `sampleCoverage.test.ts`) and `snap()` each provider surface —
  starting with the Binary Inspector's tabs, and covering the other providers that render structured UI.
- Capture each surface in **both light and dark theme**, and at a **narrow pane width** as well as a
  comfortable one — the narrow case is where clipping and pill-reflow defects actually appear.
- Ensure the screenshot artifact upload includes these (note: the workflow needs `include-hidden-files: true`,
  since dot-directories are excluded by default — this has bitten the crew before).
- Wire the new specs into the existing ratchet so a newly-broken surface fails rather than quietly drops.
- Document, in the harness README, the one-line way to add a spec for a new provider — the point is that
  the next preview feature ships its screenshot coverage as a matter of course.

## Acceptance criteria
- A CI run on a PR touching the preview pane uploads screenshots of the affected provider surfaces, in
  both themes, without anyone building a bespoke harness.
- The Binary Inspector's `.NET metadata` tab specifically is covered.
- The ratchet's known-failing baseline is not silently raised to absorb new specs.
- The burndown row in `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` flips to done, naming the CI job
  that pins it.

**Conflict surface:** the `gui-smoke` harness directory (specs + README), the GUI smoke workflow file, and
`.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`. Independent of feature work.
