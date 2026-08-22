---
id: CPE-1862
title: retention prunes manifests but nobody reconciles checkpoints.json, leaving dangling rows
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-22
closed:
---

## Problem

`checkpoints.json` is an **append-only index that nothing reconciles**. Retention prunes manifests
(`snapshot_prune::apply`, reached from `checkpoint_prune_apply` at
`crates/server/src/checkpoint_store.rs:500-508`), but the rows naming those manifests stay.

So the UI lists checkpoints whose manifest is gone. Selecting one gets an error from `load_manifest`
rather than a row that was never offered.

## Why it is filed separately

CPE-1861's review found this while confirming the change did not touch it. It was originally noted as
belonging with **CPE-1845**, and the reviewer disagreed with that home for a good reason:

- CPE-1845 is about `OpResult` lacking a structural flag to separate a deliberate hold-back from a real
  failure — a **result-shape** defect.
- This is an **index nobody reconciles** — a different defect that happens to live in the same file.

More practically: CPE-1845's own ticket carried no record of it, so the note existed only in CPE-1861's
Work Log and PR body and would have been lost. A search of the open queues found no existing ticket
mentioning `checkpoints.json`.

## Acceptance criteria

- [ ] After a retention pass, `checkpoints.json` contains no row whose manifest is gone — either
      reconciled at prune time, or filtered at read time. Decide which and record why; a filter at read
      time leaves the file growing, a reconcile at prune time makes retention a writer of a file it
      currently only reads through.
- [ ] A checkpoint the user can see is a checkpoint they can act on. Pin that: list, then act on every
      row listed, and assert none errors with a missing manifest.
- [ ] Check what happens to a row whose manifest is present but **unloadable** — CPE-1861 established
      that `list_manifests` now skips a manifest disagreeing with its filename, failing
      `validate_manifest_id`, or contradicting its own `file_count`. Such a manifest is never pruned, so
      its row stays valid-looking while retention ignores the file entirely. Say whether the UI should
      show it, and what it should say.
- [ ] Red-proof each test with the minimal realistic change, observe red, revert, record the line.
- [ ] Assert each fixture is live — that the prune actually removed the manifest — before asserting the
      row's state. Six inert tests were caught on CPE-1823 and one ordering bug on CPE-1861, both because
      an assertion stood in for the thing it was meant to observe.

## Notes

Confirmed pre-existing and untouched by CPE-1861: `checkpoint_prune_apply` is a two-line pass-through and
is not in that diff at all; its whole `checkpoint_store.rs` change sits inside `mod tests`.

Read CPE-1861's Work Log before starting — it carries the identity rules that decide which manifests
retention will and will not act on, which is exactly what determines whether a row can go stale.

Related: CPE-1845 (the `OpResult` discriminant, same file, different defect), CPE-1861 (the identity
rules), CPE-1844 (`index.json` steering prune — the other unreconciled store file).
