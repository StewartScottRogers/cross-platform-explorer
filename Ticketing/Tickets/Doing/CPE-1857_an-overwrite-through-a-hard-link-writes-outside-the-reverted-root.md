---
id: CPE-1857
title: an Overwrite through a pre-existing hard link writes outside the reverted root
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-22
closed:
---

## Problem

`fs::copy` writes **through an existing inode**. If an in-tree name is a hard link to a file outside the
reverted root, an `Overwrite` rewrites the outside file.

Measured by the independent Security Auditor during CPE-1823's round-5 audit, through the registered
command:

```
CMD revert[hardlink out-of-tree]: applied=1 skipped=0
      h.txt   = Ok("CHECKPOINT-H")
      outside = Ok("CHECKPOINT-H")     <- outside the reverted root
```

`canonicalize` cannot see hard links — a hard link is not a reparse point and has no "target" to resolve —
so neither `confined_to` nor CPE-1823's new `landing` can detect it. Both correctly report the path as
inside the root, because it **is**.

## The precondition, stated precisely

A planted manifest **cannot create** the hard link. It can only **aim at one that already exists**.

But the aiming half is fully attacker-chosen: the manifest controls both the path and the blob hash, so
given any pre-existing in-tree hard link, an attacker picks which of the store's blobs lands on the far
end of it.

## Why Medium

It needs a pre-existing hard link, which is not something the threat model can manufacture. Against that:
hard links occur naturally (deduplicating backup tools, package managers, some sync clients), the threat
premise CPE-1823 established already covers a store copied between machines or synced by a cloud client,
and the write lands somewhere the user never named.

This is a property of **every file writer in the crate**, not of CPE-1823's changes — it is already
recorded on `confined_to`'s own doc as a known limit. CPE-1823 correctly did not treat it as a regression.

## Acceptance criteria

- [ ] Decide and record whether writes should refuse a target with a link count above 1, or open with a
      flag that refuses to follow, or accept the limit explicitly. Each has a real cost — refusing on link
      count would break legitimate deduplicated trees — so the decision matters more than the mechanism.
- [ ] Whatever is chosen applies to **every** writer in the crate, not just the revert path. Enumerate them
      first: `revert_engine::apply_write`, `snapshot_capture::restore`, the transfer and archive writers,
      and anything else `fs::copy`/`fs::write` reaches. A partial sweep presented as complete is this
      repo's most-repeated defect.
- [ ] A test stages the exact shape above — in-tree name hard-linked to an out-of-tree file — and asserts
      the outside file's bytes are unchanged **before** asserting the `Result`.
- [ ] Red-proof with the minimal realistic change, observe red, revert, record the line. Assert the fixture
      is live first: confirm the hard link really was created (link count, or writing through one name and
      reading the other), or the test certifies nothing.
- [ ] Update `confined_to`'s doc if the limit changes, and CPE-1823's residual notes if this closes one of
      them.

## Notes

Found by the independent Security Auditor during CPE-1823's round-5 audit — the round where 24 attack
shapes were tried and none got through. It classified this as pre-existing and not a merge blocker, and
asked that it be filed rather than folded into a ticket already five rounds deep.

Related: CPE-1846 (the final-component link swap, reducible with the crate's existing NOFOLLOW pattern),
CPE-1847 (the emptied manifest), CPE-1823 (the guards this sits beside).
