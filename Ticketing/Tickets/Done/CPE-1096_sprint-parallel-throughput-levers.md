---
id: CPE-1096
title: Sprint — lift parallel-throughput ceilings (ready bench, measured parallelism, batch-merge)
type: Task
status: Done
priority: Medium
component: Docs
tags: [ready]
estimate: 30m
created: 2026-07-26
closed: 2026-07-26
---

## Summary

The sprint skill (`.claude/commands/sprint.md`) capped total useful throughput at three
distinct ceilings, and its defaults addressed none of them explicitly: (1) the serial merge lock +
single Foreman attention, (2) machine hardware — concurrent `cargo build`s thrashing the box, and
(3) the supply of *ready, independent* tickets to parallelise over. This ticket revises the skill's
Capacity & throughput and per-ticket-pipeline sections so each ceiling has an explicit lever the
metrics ledger can drive.

## Acceptance Criteria

- [x] Ceiling #3 (work supply): a "Keep a deep ready bench" dispatch rule — keep ≥(target
      parallelism) tickets pre-sliced and tagged with their conflict surface; decompose the next epic
      ahead of need; favour slicing along the `cpe-server`/crate seam.
- [x] Ceiling #2 (hardware): "Right-size parallelism" rewritten to key off *measured* cores/RAM, cap
      total agents near `min(cores − 2, ready-independent-tickets)`, and cap concurrent Rust builds
      separately-and-lower with staggered dispatch to avoid build thrash.
- [x] Ceiling #1 (merge bottleneck): Reviewer/UAT return machine-checkable verdicts so the Foreman
      merge is a rubber-stamp on evidence; disjoint green PRs are batch-merged back-to-back; the
      "merge queue backing up?" checkpoint calls for a batch-merge drain pass.
- [x] Edits are self-consistent with the existing metrics ledger (which diagnoses *which* ceiling is
      binding on a given shift).

## Resolution

Edited `.claude/commands/sprint.md`:

- **Capacity & throughput → At each dispatch:** added the **"Keep a deep ready bench"** bullet and
  rewrote **"Right-size parallelism"** into **"Right-size parallelism to *measured* capacity"** with a
  separate, lower cap on concurrent Rust builds + staggered dispatch.
- **Per-ticket pipeline:** added a paragraph requiring **machine-checkable verdicts** from Reviewer
  (`APPROVE` / `CHANGES REQUESTED`) and UAT (`UAT PASS` / `UAT FAIL` with commands + observed output)
  so the merge is a rubber-stamp, plus **batch-merge of disjoint green PRs**.
- **At each idle checkpoint → "Merge queue backing up?":** now specifies a **batch-merge drain pass**.

No app code touched — this is a process/skill doc change. The metrics ledger already in the skill is
the diagnostic that tells a shift which of the three ceilings is currently binding, so these levers
are data-driven rather than fixed guesses.

## Work Log

2026-07-26 — Filed from a live design discussion on increasing sprint parallel throughput.
2026-07-26 — Framed the three throughput ceilings (serial merge / hardware / work supply); user asked
             to install concrete edits addressing each.
2026-07-26 — Applied three edits to `.claude/commands/sprint.md`; wrapped in this ticket at user
             request. Closed Done.

## Notes

Cross-cutting habit memories touched by this change: `[[concurrent-nightshift-coordination]]` (merge
coordination), `[[verify-subagent-merges]]` (verify a merge landed), and the crate seam described in
`docs/design/SERVER-ARCHITECTURE.md` (the architectural throughput lever for ceiling #3).
