---
id: CPE-1891
title: one occupied name now aborts and rolls back a whole macro batch, with no way to say "yes, overwrite"
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-25
closed:
---

## Problem

CPE-1734 made macro Convert **refuse** a destination that is a link *or* a plain occupied name,
rather than silently writing through / clobbering it. That refusal is correct and it closed a real
data-loss path.

But two pre-existing properties combine badly with it:

1. **`macro_apply_run` is strictly all-or-nothing.** Any `Err` from `macro_apply_op` aborts the whole
   run immediately and replays every already-applied op's inverse in reverse.
2. **There is no confirm-and-retry path.** `MacroRunConfirm.svelte` renders the raw error string with
   no affordance to say "yes, overwrite" and continue.

So converting 200 files where file #150 happens to collide now **fails wholesale and rolls back the
149 conversions that already succeeded**, leaving the user with an error string and no route forward
short of finding and renaming the colliding file by hand.

Before CPE-1734 that batch completed, silently clobbering one file. That was a worse default — this
ticket is not an argument to restore it — but the user has gone from "one file quietly overwritten"
to "nothing done, and no way to proceed."

## The parity that was claimed, and the half that is missing

CPE-1734's reasoning was that refusing matches the Batch-Media engine, which already refuses an
unconfirmed in-place overwrite. True as far as it goes — but Batch-Media's refusal comes **with an
escape hatch**: `overwritesInPlace()` → a confirm panel → `confirmOverwriteJob()` →
`confirmed_overwrite: true`, plus a pre-overwrite checkpoint. The macro engine has the refusal and not
the hatch.

Found by PR #1025's reviewer, which checked the parity claim rather than accepting it.

## What to do

Decide between these deliberately, and record why — they are genuinely different products:

1. **Give the macro engine Batch-Media's confirm path.** Most consistent, most work: surface the
   collision, let the user confirm, take the same pre-overwrite checkpoint, continue.
2. **Pre-flight the run.** Check every destination *before* applying anything, and present the whole
   collision set up front — "these 3 of 200 will be skipped / need confirmation" — rather than
   discovering it at file #150. This fits the all-or-nothing contract rather than fighting it.
3. **Make Convert's refusal skippable per entry** while keeping the abort for genuine errors. This
   breaks the documented all-or-nothing macro contract, so it needs the strongest justification of
   the three.

**Whichever you choose, the user must be able to see which names collided.** Note **CPE-1869** just
landed a copy-the-full-list affordance on the revert panel for exactly this shape of problem — a list
the user is told to act on but cannot see. Reuse the approach rather than inventing a second one.

## Not in scope

The all-or-nothing rollback itself is the documented, deliberate macro contract and predates this. Do
not change it as a side effect; if option 3 is chosen, that is a deliberate contract change and must
be argued in the work log.

## Acceptance criteria

- [ ] A 200-file convert with one colliding name no longer loses the other 199 conversions with no
      recourse — demonstrated end to end.
- [ ] The user can see *which* names collided.
- [ ] Whatever path is chosen, the link/write-through refusal from CPE-1734 stays absolute — a confirm
      may allow overwriting a **plain file**, never writing through a link.
- [ ] The chosen approach is recorded with its reasoning against the other two.

## Work Log

- **2026-08-25 17:15 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`, from
  PR #1025's reviewer. It approved that PR and flagged this anyway, having traced the parity claim into
  `batchMedia.ts` and found the confirm path the macro engine lacks. The bar it applied — "the PR body
  does not mention this interaction, and it will surprise a user converting 200 files where file #150
  collides" — is the right one.
