---
id: CPE-1768
title: The render guard covers 41 of 136 components, and nothing states which files must be registered
type: task
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

Measured by the **PR #925 (CPE-1761) review** while establishing the guard's blast radius.
`REGISTRY` in `src/lib/bidiEscape.guard.test.ts` covers **41 of the 136 `.svelte` files** under `src/`.
The other 95 are **never scanned**.

Demonstrated: injecting a lone `{` **and** a raw `{entry.name}` render into `StatusBar.svelte` left the
guard **green** — because `StatusBar.svelte` is not registered, so it is not scanned at all.

The coverage number is not itself the bug. The bug is that **nothing writes down which files must be
registered, or why**, so:

- A new component that renders a filesystem name can be added and never registered, and no test notices.
  The guard's protection is opt-in, and opting in is a step a person has to remember — which is precisely
  what a guard exists to remove.
- The module header (`src/lib/bidiRenderScan.ts:28-29`) claims a run "either names every offender or
  refuses to answer; it can never silently claim a file is clean when it wasn't sure." Read against 41/136
  coverage, that promise is much broader than what the guard actually makes. An over-confident header on a
  security guard is its own hazard.

## What to do

Decide and record the membership rule, then enforce it mechanically:

- **State the criterion.** Most likely "every component that renders a value derived from a filesystem name
  or path". Write it in the module header, in plain terms, with an example of an in-scope and an
  out-of-scope component.
- **Enforce it.** A guard test that finds candidate components automatically — e.g. any `.svelte` file
  referencing `entry.name`, `entry.path`, a `name`/`path` prop, or the known name-bearing stores — and
  fails when one is not in `REGISTRY`. Registration should become the thing you cannot forget, rather than
  the thing you must remember.
- **Expect a backlog.** Running that discovery over the 95 unscanned files will surface components that
  should have been registered. Triage each individually — escape the genuinely unsafe, record the benign
  with their expression. **Do not bulk-register the difference**; that launders real offenders into the
  registry, which is the failure this guard family keeps repeating.
- **Correct the header's promise** to match the real scope, whatever it turns out to be.

## Acceptance criteria

- [ ] The membership criterion is stated in the module header with an in-scope and an out-of-scope example.
- [ ] A test fails when a component meeting the criterion is absent from `REGISTRY`. Demonstrate by adding a
      new component that renders a raw name and showing CI red without any manual registration step.
- [ ] The discovery pass over all 136 files is run and its results reported: how many candidates, how many
      already registered, and each newly-found one triaged individually with its verdict.
- [ ] `StatusBar.svelte`'s injected-render probe from the review now reds (or `StatusBar.svelte` is
      explicitly out of scope by the stated criterion, with the reason recorded).
- [ ] The module header's coverage claim matches measured reality.
- [ ] Breaking the membership test reds a **distinct** test naming the unregistered file.

## Notes

Measured by the Reviewer on **PR #925 / CPE-1761**, 2026-08-17, during the batched sprint; the review judged
it out of scope for that PR and closer to CPE-1757's. Related: CPE-1757 (the parser rewrite that built the
registry), CPE-1761, CPE-1766, CPE-1767, CPE-1712.
