---
id: CPE-1844
title: a hand-edited index.json steers prune into deleting the user's other checkpoints
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-21
closed:
---

## Problem

`store_total_bytes` (`crates/server/src/snapshot_capture.rs:560`) reads its figure from `index.json`.
The retention byte cap (`crates/server/src/snapshot_prune.rs:107-121`) turns that number into real
`prune` deletions of the user's **other** checkpoints, floored at one survivor.

`index.json` is exactly as hand-editable as the manifest that CPE-1823 spent four review rounds
hardening, and it receives **none** of that ticket's validation. Inflate the recorded total and the
retention policy concludes the store is over its cap and starts deleting checkpoints that should have
been kept.

This is the same shape as CPE-1823 — *a hand-editable file steers a destructive decision* — one file
over.

## Why it is Medium rather than High

The damage is confined to the snapshot store: it deletes checkpoints, not user data, and the floor
guarantees one survivor. It also needs the same precondition as CPE-1823 (write access to the store),
and the same threat premise: a store copied between machines, restored from a shared drive, or synced
by a cloud client.

But losing checkpoints silently is losing the user's ability to undo — and CPE-1823 established that
this store's inputs are not trustworthy.

## Acceptance criteria

- [ ] `index.json`'s numeric fields are validated or recomputed before any retention decision uses them.
      Prefer **recomputing** from what is actually on disk over validating a claim — CPE-1823's diff cap
      had the same shape (it gated on the manifest's claimed `size` and was defeated by a manifest
      claiming `size: 1`), and the fix there was to measure the real thing, not to sanity-check the
      claim. If recomputing is too expensive to do every time, say what it costs and gate it.
- [ ] A prune driven by a tampered or stale `index.json` cannot delete a checkpoint that the real
      on-disk state says should be kept.
- [ ] Every other field of `index.json` that reaches a decision is enumerated and either validated or
      explicitly recorded as harmless. CPE-1823 found its third, fourth and fifth sinks by enumerating
      rather than trusting the ticket — do the same here.
- [ ] Tests stage a tampered `index.json` for each shape and assert **the harm did not happen** —
      the checkpoint that should survive is still there — before asserting the `Result`.
- [ ] Red-proof each test with the minimal realistic change, observe red, revert, record the line.

## Notes

Found by the independent Reviewer during CPE-1823's round-3 review, which correctly declined to absorb
it — nothing in that PR made this worse, and scope creep on a ticket already four rounds deep would
have been the wrong call.

Read CPE-1823's final Work Log before starting. It carries the attack record that matters here: which
shapes defeat textual checks, why `canonicalize` cannot see a hard link, and the rule that a guard
belongs where callers inherit it rather than at each call site. That ticket needed four rounds largely
because guards kept landing on the path with no callers while the shipping path went unguarded — check
which functions here are actually reachable from a registered command before deciding where to put
anything.
