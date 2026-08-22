---
id: CPE-1847
title: a planted zero-entry manifest deletes the whole tree and reports complete success
type: bug
priority: High
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

## Why High

Every other manifest attack CPE-1823 closed required a crafted key that survived a guard. This one
requires **deleting text**. It is the cheapest possible tamper — truncate the map to `{}` — and its blast
radius is the entire tree rather than one named file.

Mitigation, which is real but partial: the UI previews before reverting (`AgentTimeline.svelte:483`,
`CheckpointDialog.svelte:138`), so an attentive user sees five deletes and no creates before confirming.
But `checkpoint_revert` is callable without the preview, and "the UI happens to ask first" is not a
guard — it is a habit.

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

Note a disagreement between CPE-1823's two checkers, which the round-5 Work Log is settling: the Reviewer
called this the widest destructive shape a planted manifest has left; the Security Auditor argued the
case-alias is wider, having a strictly narrower precondition and needing no whole-tree-wipe confirmation.
Both are open, both are worth closing, and the ranking does not change the work here.

Related: CPE-1823 (the guards this evades), CPE-1844 (`index.json` steering prune, the same
hand-editable-file-steers-a-destructive-decision shape), CPE-1845 (the reporting discriminant).
