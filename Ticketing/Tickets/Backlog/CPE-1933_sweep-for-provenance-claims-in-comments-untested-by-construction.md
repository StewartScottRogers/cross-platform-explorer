---
id: CPE-1933
title: sweep for **provenance claims in comments** — "mirrors `release.yml`", "same as X" — which are untested by construction and decay silently
type: task
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## The pattern

From CPE-1917's worker, generalising a defect it found *inside the fix for the bug it was sent to
investigate*:

> The defect was not a wrong assertion — the tests pass and prove something real. It was a **doc
> comment asserting provenance**: *"exactly the way `release.yml` invokes it"*. Provenance claims in
> comments are **untested by construction**, and they are the ones that decay silently, because the
> test staying green reads as the claim staying true.

That last clause is the whole problem. A stale provenance comment is worse than no comment, because
the surrounding green test actively vouches for it.

## How it presented

CPE-1872's `release_guard.rs` carried three claims that it mirrored `release.yml`'s invocation. They
were **true for exactly one commit**. Round 2 of that same PR moved the check out of the build matrix
into a post-matrix job and changed the arguments — and left the hard-coded argv, and its claim,
behind. Every test kept passing. Nothing could have caught it, because nothing derived the claim from
the thing it claimed to mirror.

The fix shape is the one CPE-1917 used and it is the general answer: **either derive it from the
source at runtime, or do not claim it.** Its new `release_workflow_wiring.rs` reads the invariant's
two halves out of `release.yml` from two different places and executes them against each other, so it
cannot rot the same way.

## Acceptance criteria

- [ ] Sweep for live instances. Suggested starting point, from the worker who found it:
      `grep -rn "exactly the way\|same as \|mirrors \|matches the workflow\|kept in sync with" --include=*.rs --include=*.ts --include=*.mjs --include=*.svelte`
      Expect a handful. Broaden the phrasing once you see what the real ones look like — this list is
      a seed, not the enumeration.
- [ ] For each hit, classify: **derive it** (read the referenced source at runtime and assert against
      it), **delete the claim** (keep the code, drop the assertion of provenance), or **genuinely
      unavoidable** — and if the last, say at the site that the claim is unverified and why, so a
      future reader treats it as folklore rather than fact.
- [ ] Prioritise by blast radius. A stale provenance claim in **release plumbing** or a **security
      guard** is worth deriving properly; one in a UI helper may be fine to simply delete.
- [ ] Red-proof each derivation: change the referenced source and confirm the deriving test goes red.
      A derivation that does not actually re-read its source is the same defect with extra steps.
- [ ] Record the pattern wherever this repo keeps its testing guidance, so the next person writing
      "same as X" reaches for a derivation instead.

## Notes

Filed 2026-08-27 by the sprint Foreman from CPE-1917's worker. Sibling of **CPE-1929** (shadowed
guards — a guard that cannot go red because an earlier one answers first) and **CPE-1932** (rules
followed from memory rather than enumerated). All three are the same family: **a claim that looks
checked and is not.**

Related: **CPE-1872** (where the stale claims lived), **CPE-1917** (which corrected three of them and
built the deriving guard).
