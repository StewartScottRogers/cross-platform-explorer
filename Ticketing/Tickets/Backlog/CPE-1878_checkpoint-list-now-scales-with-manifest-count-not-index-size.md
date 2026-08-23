---
id: CPE-1878
title: checkpoint_list now scales with manifest count, not index size — 33x slower at 300 checkpoints
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-23
closed:
---

## Problem

CPE-1862 gave `checkpoint_list` a read-time filter against `snapshot_capture::list_manifests`, so a
row whose manifest is gone — or present but unloadable — is never offered to the user. That fix is
correct and it merged. This ticket records what it cost, because nobody would otherwise notice until
it hurt.

Measured by PR #1014's independent UAT on a store of **300 checkpoints**, with a no-op retention
policy so only the fix's own overhead is timed:

| | before (`main`) | after |
|---|---|---|
| `checkpoint_prune_apply` | 21.4 ms | 31.8 ms |
| `checkpoint_list` | **0.87 ms** | **28.9 ms** |

`checkpoint_list` is ~33× slower because it now re-reads and parses **every manifest file on disk**
on every call, rather than the single small `checkpoints.json` index.

## Why this is Low, and why it is filed anyway

29 ms is imperceptible in a dialog, and at today's realistic checkpoint counts nobody will feel it.
Nothing is broken.

But the change is **algorithmic, not constant**: list cost now scales with the number of manifests
rather than the size of the index. At 3,000 checkpoints that is ~290 ms on every open of the
checkpoint dialog and every Agent Timeline refresh that calls it — and PURPOSE.md's tiebreaker is
fast / small / predictable. An unrecorded algorithmic change is the kind of thing that gets
rediscovered a year later as "the checkpoint dialog got slow at some point".

Every consumer goes through `checkpoint_list` — `copilot.rs`, `organize_apply.rs`,
`snapshot_schedule.rs`, the `checkpoint_list` Tauri command, and both frontend readers
(`CheckpointDialog.svelte`, `AgentTimeline.svelte` via `commands.checkpointList`) — so the cost is
paid everywhere, not in one dialog.

## What to do

Do **not** revert the filter — it is the backstop that makes the crash window between "manifests
deleted" and "index reconciled" harmless.

Options, in rough order of preference:

1. **Make the filter cheap.** `list_manifests` currently reads and parses each manifest. Establishing
   that a manifest *exists and loads* may not need a full parse — a directory listing plus an
   existence check covers the "deleted" case, which is the common one; the "present but unloadable"
   case (CPE-1861 identity) is rarer and could be checked lazily, only for rows about to be acted on.
2. **Cache with invalidation** keyed on the manifests directory's mtime, so repeated `checkpoint_list`
   calls in one session pay once.
3. **Measure first and do nothing** if a realistic upper bound on checkpoint count keeps this under a
   threshold you are willing to write down. That is a legitimate outcome — but write the number down.

Whatever is chosen, **add a benchmark or a test that fails if list cost regresses past a stated
bound**, so the next algorithmic change to this path is not invisible either.

## Acceptance criteria

- [ ] A decision recorded, with a number: the checkpoint count this path is expected to handle and the
      latency it must stay under.
- [ ] If optimised: the same correctness tests from CPE-1862 still pass, red-proofed.
- [ ] A guard that fails on a future regression past the stated bound.

## Work Log

- **2026-08-23 16:42 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from PR #1014's UAT, which measured this without being asked to and reported it plainly rather than
  letting a green suite speak for it. CPE-1862 merged on the strength of its correctness; this is the
  cost note, not an objection to it.
