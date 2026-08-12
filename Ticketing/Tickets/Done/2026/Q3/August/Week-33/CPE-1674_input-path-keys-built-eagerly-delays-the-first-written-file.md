---
id: CPE-1674
title: Batch Media canonicalises every input before the first file is written, even when the keys are never consulted
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Raised by the independent Reviewer on PR #856, twice — the second time on a ground that changes the verdict.

`batch_execute::input_path_keys` builds a `HashSet<PathKey>` of every input in the batch **once, before the
loop starts**. That is the right shape for the security fix it was introduced for (CPE-1667: it replaced an
O(n) scan that ran *inside* the verify→write window). But it runs **unconditionally**, including when
`confirmed_overwrite == true` — where the keys are never consulted, because the foreign-overwrite question
is short-circuited.

The first framing was "total batch cost", which is a fair trade: n canonicalize calls against n image
transforms is nothing. The framing that actually matters is different: those n canonicalize calls all
happen **before the first file is written**, so on a large batch over a network share they delay **time to
first written file**. That cuts directly against this repo's streaming-liveness convention
(`docs/design/STREAMING.md`) — paint the first result immediately rather than making the user wait for a
whole collection pass.

## Scope

Build the set lazily — a `OnceCell` populated on first use — so the confirmed-overwrite path pays nothing
and the ordinary path pays only when it first needs an answer, after the first item is already moving.

Do **not** move the construction back inside the per-item window: the whole point of CPE-1667 was to get it
out of there, and the security property (an O(1) in-window check) must be preserved. Lazy-once-per-batch is
compatible with that; per-item is not.

## Acceptance criteria

- [ ] A batch with `confirmed_overwrite = true` performs **zero** canonicalize calls for `input_path_keys` —
      measured with the existing deterministic counter, not inferred.
- [ ] The in-window cost stays O(1) — `cpe_1667_is_foreign_overwrite_costs_a_bounded_number_of_canonicalize_calls_regardless_of_batch_size`
      must remain green and unchanged.
- [ ] Time to first written file on an ordinary (non-confirmed) batch is no worse than today, and measurably
      better on a large one. Measure with a control, as CPE-1667 did.
- [ ] Removing the laziness turns the new measurement test red.

## Notes

Filed by the Foreman from the PR #856 re-review, 2026-08-12. The reviewer had raised it as a nit in the
first round and I left it; it re-raised it with the streaming-liveness argument, which I accept.

Related, and worth reading first: **CPE-1667** (the ticket this came out of) and `WindowTrace`'s doc comment
in `crates/server/src/batch_execute.rs`, which records what the window guard does and — more importantly —
what it deliberately does not cover.
