---
id: CPE-1928
title: the two link-hazard sentences read as duplicates, because the only differing clause is buried mid-sentence
type: task
priority: Low
status: Open
tags: ready
estimate: XS
created: 2026-08-27
---

## Summary

When a macro run hits **both** link-hazard kinds at once, the macro run-confirm dialog shows two
explanation sentences that open identically ("This destination is a link, and…") and close
identically ("…remove the link first if that is what you meant"), differing only in the middle
clause. That puts roughly seven lines of near-duplicate prose above a three-line list, with the
differentiator buried where the eye does not land. At a glance it reads as the same paragraph twice.

Observed by PR #1044's Visual Critic in `.claude/sprint-metrics/visual-evidence/cpe-1891-light-many-blocked.png`
(three blocked collisions across two hazard kinds).

## Decision taken, and why it is not in CPE-1891

The Critic offered this as a taste call. **The Foreman took the better option rather than queuing a
third question for the user, and deferred it here rather than spending a fifth round on CPE-1891**,
which was otherwise finished and holding CI capacity for four sibling PRs. The condition is
uncommon — it needs one run to hit both a rename/move link *and* a convert link.

**Take option B:** lead each sentence with the differentiator and state the shared remedy **once**
beneath both.

- "Renaming onto a link destroys it — the link is removed and its target is left orphaned."
- "Creating a file at a link's name writes THROUGH it — the bytes would land at the link's target, a
  path you did not name, and a failure part-way would then delete the link itself."
- then, once: "Nothing was changed; remove the link first if that is what you meant."

That cuts the box by about two lines and puts the distinguishing words where the eye lands.

## Acceptance criteria

- [ ] Restructure the two hazard sentences as above: differentiator first, shared remedy stated once.
- [ ] Keep `genericizeReason()`'s property — **no sentence may name any single collision's path**.
      That was CPE-1891's own fix for a real defect (one path quoted while several were listed) and
      it must survive; the existing test asserting it should still pass unchanged.
- [ ] Handle the single-hazard case gracefully — with only one kind present, the remedy must still
      read naturally rather than as an orphaned line.
- [ ] Re-capture `cpe-1891-{light,dark}-many-blocked.png`, the only evidence that exercises both
      kinds at once.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1044's Visual Critic. Purely editorial — nothing
here changes behaviour, and the Critic passed the PR with this noted as non-blocking.

Related: **CPE-1891** (the collision panel), **CPE-1892** (the copy-held-back-paths button's rough
edges), **CPE-1869** (the list-copy pattern both reuse).
