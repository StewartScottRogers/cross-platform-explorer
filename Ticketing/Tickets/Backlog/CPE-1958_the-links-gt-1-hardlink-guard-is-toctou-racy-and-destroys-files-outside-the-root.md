---
id: CPE-1958
title: `overwrite_confirmed_no_follow`'s `links > 1` hard-link guard is TOCTOU-racy — measured destroying a file outside the root
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Found by PR #1066's independent Security Auditor while verifying the CPE-1929 guard reorder. It is
**pre-existing** — reproduced against a byte-for-byte replica of `main`'s function body in the same
run, at roughly **double** the rate — and the auditor was explicit that it is not that PR's fault.

`fsutil::overwrite_confirmed_no_follow` refuses a destination whose handle reports
`nNumberOfLinks > 1`, on the reasoning that a hard link means another name shares the object. The
check reads a fact that an attacker can change **after** the open and **before** the read.

**Attack:** with write access to the destination directory, loop
`hard_link(outside_victim, slot)` / `remove_file(slot)` while a confirmed Convert writes to `slot`.

**Measured on disk:** the outside victim `RACE_VICTIM.txt` held `ATTACKER PAYLOAD` instead of
`UNTOUCHED`.

| implementation | hard-link-only swapper | mixed swapper |
|---|---|---|
| PR #1066 (guard moved ahead of the path check) | **17 destroyed / 1,000** | 5 / 2,000 |
| `main`'s body, replicated in the same run | **30 destroyed / 1,000** | 27 / 2,000 |

**PR #1066 halves the window but does not close it** — moving `links > 1` ahead of the path check
shrinks the interval, it does not remove the check-then-use.

**Mechanism** (interleaving inferred, effect measured): the open lands on the existing hard link, so
the handle is the victim's object; the attacker unlinks the second name before `handle_facts` runs;
`nNumberOfLinks` reads 1; the guard passes; `set_len(0) + write_all` lands on the victim.

**The racer was proven sensitive**, in the CPE-1937 shape: with all four guards disabled the same
racer destroys the victim, and statically the hard-link leg returns `Ok` with `victim="NEW"`.

**`batch_media::open_output_verified` under the identical racer: 0 destroyed in 2,000 trials.**

## Why re-checking harder will not fix it

The auditor's diagnosis, and it is the load-bearing part: **re-checking the same racy fact does not
help.** `nNumberOfLinks` is a property of the object at the moment it is read, and any number of reads
can each be true and stale. Two shapes that do work:

- **Claim-then-rename** — take the destination under a name only this operation owns, so no second
  name can be attached to the object between the check and the write.
- **Post-write re-verify** — after writing, confirm by **handle identity** that the object written is
  the object claimed, and undo if not.

Both are real designs, not one-line fixes, which is why this is its own ticket rather than a rider.

## Acceptance criteria

- [ ] **Reproduce first, with the auditor's racer.** Do not start from the fix. Report destroyed/trials
      for the current code, and keep the **sensitivity control** — with the guards disabled the racer
      must destroy the victim, or the harness proves nothing (CPE-1937's lesson).
- [ ] Pick claim-then-rename or post-write re-verify, **record why**, and say what it costs. A design
      that closes the window is worth more than one that narrows it further.
- [ ] **Assert on the filesystem** — the outside victim byte-identical — never on a verdict enum. This
      family's whole history is reports that look healthy while files are destroyed.
- [ ] Red-proof by racing, not by reading. Report before/after at comparable trial counts.
- [ ] Check the **other** `links > 1` sites for the same shape. `batch_media::open_output_verified`
      measured 0/2,000 under the identical racer — establish *why* it is safe and whether that property
      can be moved to `fsutil`, rather than treating the difference as luck. Enumerate rather than
      recall (CPE-1932).
- [ ] While there: PR #1066's Auditor notes the two sites now state **opposite doctrines** about a
      non-surrogate reparse point — `fsutil` **writes** it (CPE-1896's dehydrated-placeholder rule),
      `batch_media::open_output_verified` **refuses** it on the bare bit, now cemented by a new test.
      Neither is wrong, but only one is documented as a deliberate choice. Record the split or unify it.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1066's Security Auditor (**SEC PASS** — it did not
block on this, since the PR halves the rate rather than causing it).

Family: **CPE-1896** (the handle gate), **CPE-1913** (the containment gates), **CPE-1937**
(`remove_file_beneath`, and the racer shape this used), **CPE-1929** (the reorder that surfaced it, PR
#1066), **CPE-1957** (the three shadowed sites left unmeasured).


## Correction 2026-08-27 — `batch_media` is NOT safe, and this ticket's premise was wrong

This ticket said `batch_media::open_output_verified` *"measured 0 / 2,000 under the identical racer"*
and asked whoever fixed `fsutil` to establish **why it was safe** rather than treating the difference
as luck. That framing came from the Foreman and PR #1070's worker disproved it.

**It is not safe — it is *shielded*, and only on Windows.** `classify_output_containment` runs
**before** the open and refuses a flickering destination outright, so far fewer trials reach the
identical check-then-use (681 `Ok` vs this site's 1,249 in the same run). A **path gauntlet, not
containment.** Linux has no such shield and measures **~30 / 1,000**.

That is CPE-1929's shape: a guard that survives because an earlier check happens to reject most of the
attacker's attempts looks like a property and is a coincidence. **The instruction to find out why it
was safe was the right instruction; the premise it rested on was not.**

`claim_destination_handle` (`fsutil.rs:1734`) is live too — **45 / 2,000** Windows, **90 / 1,000**
Linux. Both are now owned by **CPE-1961**.
