---
id: CPE-1869
title: the held-back list tells you to delete files it will not show you
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-23
closed:
---

## Problem

CPE-1845's revert panel names up to **8** held-back paths, then prints "and N more". Its next step, in
the permanent cases, says **"delete these files yourself if you want them gone"**.

At 200 held-back deletions the user is told to act on a set they can see 4% of.

Whether that matters depends entirely on which hold-back fired:

- **Empty checkpoint** — survivable. The held-back set is literally everything in the folder, so the user
  can see it in the file pane.
- **Unrestorable key** — **not** survivable. The set is "everything added since the checkpoint", which is
  not derivable from anything on screen and appears nowhere else in the app.

So the advice is actionable in one case and a dead end in the other, with identical wording and an
identical 8-name preview.

## What it needs

Not a bigger cap. Both the UAT and the worker landed on the same answer independently: **8 is a fine
preview provided the full list is retrievable.** What is missing is an affordance —

- copy the full list to the clipboard, or
- reveal/select the held-back paths in the file pane, or
- write them to a file the user can open.

The file-pane route is the strongest, because the user's next action is deleting them.

## Acceptance criteria

- [ ] The full held-back set is obtainable without re-running the revert. Say which affordance you chose
      and why.
- [ ] The permanent-case next step points at that affordance rather than at a list the user cannot see.
- [ ] The 8-name preview stays. Do not replace it with a scrolling list of 200 — the count and the reason
      are what the user needs first, and CPE-1845 measured what 200 repeated lines cost.
- [ ] Check the alias/collision case does **not** get a delete affordance. Those files **are** the
      checkpoint's own content under another spelling; deleting them destroys it. CPE-1845's docs carry
      the fourth bullet for exactly this reason.
- [ ] Red-proof each new test with the minimal realistic change, observe red, revert, record the line.
- [ ] Assert the fixture is live before asserting the harm. Fold the check into helpers rather than per
      test — that is what fixed CPE-1844 after its liveness claim inverted, and CPE-1845's own first draft
      of a test passed with the fix disabled because the fixture armed a different branch.

## Notes

Recorded by CPE-1845's worker and independently by its UAT, both concluding it is a new UI surface rather
than a wording change. Written into `MAX_LISTED`'s doc comment so the next person to touch the cap finds
the reasoning before changing the number.

One limit that will apply here too: jsdom applies no component CSS under this project's vitest config, so
nothing you write can check layout, ordering on screen, or visibility. CPE-1859 built a real-render
harness (`scripts/dev-harness/statusbar-notice`) for exactly this gap — reuse it rather than asserting on
markup and calling it verified.

Related: CPE-1845 (the panel and the typed outcome), CPE-1823 (the stand-down that produces the
hold-backs), CPE-1847 (the empty-checkpoint case).
