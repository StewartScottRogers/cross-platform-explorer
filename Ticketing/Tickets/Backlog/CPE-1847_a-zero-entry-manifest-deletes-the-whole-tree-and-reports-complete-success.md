---
id: CPE-1847
title: a planted zero-entry manifest deletes the whole tree and reports complete success
type: bug
priority: Critical
status: Backlog
tags: ready
estimate: M
created: 2026-08-21
closed:
---

## Problem

A checkpoint manifest whose `files` map is **empty** describes a tree with nothing in it. Revert against
a real tree therefore plans a delete for every file, executes them all, and returns success.

Confirmed by the independent Reviewer during CPE-1823's round-4 review — measured, not reasoned:

```
empty checkpoint vs a five-file tree
  -> RestoreReport { applied: 5, skipped: [] }
  -> survivors = 0
```

Five files gone, nothing skipped, **complete success reported**.

CPE-1823's stand-down cannot help here. It arms on a checkpoint entry that cannot be restored on this
platform; a zero-entry checkpoint has no entries at all, so there is nothing to stand down on. The guard
is structurally blind to this shape.

## Why Critical

Every other manifest attack CPE-1823 closed required a crafted key that survived a guard. This one
requires **deleting text**. It is the cheapest possible tamper — truncate the map to `{}` — and its blast
radius is the entire tree rather than one named file.

### It also reaches through cherry-revert, which removes the mitigation entirely

Measured by the independent Security Auditor during CPE-1823's round-5 audit:

```
CMD revert[empty manifest]:     applied=5 skipped=0   survivors = []
CMD revert_one[empty manifest]: applied=1 skipped=0   survivors = [f1,f2,f4,f5]
```

The same emptied manifest destroys files **one at a time through `checkpoint_revert_one`**, behind a
per-file confirm that says nothing about a mass delete and **never consults `checkpoint_preview_revert`**.
So the mitigation everyone assumed — that the UI previews first (`AgentTimeline.svelte:483`,
`CheckpointDialog.svelte:138`), and an attentive user would see five deletes and no creates — does not
apply on that route at all.

On the whole-tree route the preview is still real but partial: `checkpoint_revert` is callable without it,
and "the UI happens to ask first" is not a guard, it is a habit.

## The judgement call this ticket must settle

An empty checkpoint is **legitimately representable**: capturing an empty directory produces one. So the
fix cannot simply refuse `files: {}`.

Options to weigh and decide explicitly, with the reasoning recorded:

- Require a positive assertion that the capture was of an empty tree, so an emptied map and a genuinely
  empty capture are distinguishable (a count, a checksum over the entry set, or a signed/derived field).
- Refuse a revert whose plan is **all deletes and no writes** above some threshold, without confirmation
  carrying the count.
- Recompute rather than trust — the shape CPE-1823 landed on repeatedly, and the shape CPE-1844 asks for
  on the same store's `index.json`.

Prefer whichever makes the harm impossible over whichever makes the manifest look valid.

## Acceptance criteria

- [ ] A zero-entry manifest cannot silently delete a populated tree. Whatever the chosen mechanism, the
      test asserts **the files still exist** before asserting the `Result`.
- [ ] A genuine capture of an empty directory still round-trips. This is the constraint that makes a
      naive refusal wrong — pin it.
- [ ] The all-deletes-no-writes plan shape is surfaced to the caller structurally, not only in prose.
      See CPE-1845, which is adding exactly that kind of discriminant to `OpResult`.
- [ ] Enumerate any other whole-manifest shape with the same property — valid on its face, catastrophic in
      effect — rather than fixing only the empty case. CPE-1823 found its third, fourth and fifth sinks by
      enumerating instead of trusting the ticket.
- [ ] Red-proof every test with the minimal realistic change, observe red, revert, record the line.
- [ ] Assert each new test's fixture is live (that the tamper actually took effect) before asserting the
      harm. CPE-1823 caught **six** inert tests, and in every one the fixture never reached the harm.

## Notes

Filed from CPE-1823's round-4 Reviewer findings. That review made the case for a ticket rather than a
comment: *"'recorded, not fixed' in a code comment is where round 1's colon regression also lived."*

**The disagreement between CPE-1823's two checkers is settled, and this shape won.** The Reviewer called it
the widest destructive shape a planted manifest has left; the Security Auditor argued at round 4 that the
case alias was wider. Round 5 closed the alias, and the Auditor withdrew its own position: this is the
widest remaining — and **wider than either party said**, because of the `revert_one` route above.
Raised from High to Critical on that basis (2026-08-22).

Related: CPE-1823 (the guards this evades), CPE-1844 (`index.json` steering prune, the same
hand-editable-file-steers-a-destructive-decision shape), CPE-1845 (the reporting discriminant).
