---
id: CPE-1667
title: Batch Media's write-window comment states a number one branch does not hold, and that branch is O(batch size)
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Found by the independent Security Auditor while re-auditing PR #848's handle-based rewrite (which it passed —
both previously-executed exploits are dead). Filed as residue, not as a finding: it is **not exploitable**,
since an attacker must still win a sub-millisecond race against an already-pinned object.

`crates/server/src/batch_media.rs:1589-1592` states the residual verify→write window is "two syscalls wide
(microseconds)". On the `created == true` branch that is true and measured: **174,600 ns** for a single
2000×2000 item and **97,100 ns** in a 400-item batch, against a 445.6 ms transform.

On the `!created` branch it is false. `is_foreign_overwrite` sits **inside** the window and runs
`items.iter().any(|other| same_file(&other.input, &item.output))` — O(n) over the batch, each `same_file`
building a throwaway `ParentCache` and doing up to two `canonicalize` syscalls. So that branch's window is
O(batch size) syscalls, not two.

**Why nobody caught it:** the window regression test uses a **1-item batch with `confirmed_overwrite = true`**,
which takes the `created == true` branch and never executes `is_foreign_overwrite` at all. The test pins the
branch that was already fine.

## Scope

1. **Precompute the batch's input `PathKey`s once, outside the loop**, and compare against a single
   `path_key(&item.output)` — makes the in-window cost O(1).
2. **Extend the window regression test to the `!created && !confirmed_overwrite` branch with a multi-item
   batch.** Note the shape the auditor was mid-way through when its worktree was destroyed: a naive attempt
   hits the up-front refusal and measures 0 ns. Chain the batch so item *i*'s output equals item *i+1*'s
   input, which makes the O(n) scan run to completion, return false, and the item be written.
3. **Correct the comment** to whatever is measured after (1). If a branch is still wider, say so — this crew
   has now spent three separate rounds correcting comments that stated invariants the code did not hold.

Two cosmetic items from the same PR's UAT, worth folding in while the file is open:

4. `VerifiedOutput::write_all` (`batch_media.rs` ~1611) has **no cleanup on failure**: if `set_len` /
   `write_all` / `flush` itself errors on a freshly-created output — a genuine disk I/O fault, not any
   adversarial path (those all correctly call `.abandon()`) — the empty or partial file is left on disk.
   Found by code reading; the UAT could not force a real disk-full condition to confirm.
5. The symlink-refusal message reads *"…is a link, and a batch never writes through one — a link's target can
   be re-pointed after any check."* Clear for the attack case, but a user who deliberately set up a **dangling
   symlink pointing back inside their own selected folder** — previously allowed, now refused — gets no
   acknowledgement that their case is different. One clause would do it.

## Acceptance criteria

- [ ] The in-window cost on the `!created` branch is O(1), measured.
- [ ] A regression test pins the window on **both** branches, using the chained-batch shape so the O(n) path
      genuinely executes.
- [ ] The comment states the measured numbers for both branches.
- [ ] A write that fails mid-way on a file this call created leaves nothing behind.
- [ ] The symlink message names the dangling-inside-the-folder case.

## Notes

Filed by the Foreman from the PR #848 re-audit and UAT, 2026-08-12. PR #848 merged on a SEC PASS, a reviewer
APPROVE and a UAT PASS; this is the recorded residue.

The auditor's O(n) claim is **derived from reading the code, not measured** — its worktree was destroyed by a
Foreman cleanup error before it could finish the measurement. Measure it yourself rather than taking the shape
on trust.
