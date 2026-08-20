---
id: CPE-1801
title: a slashes-only S3 key is dropped from the listing and not counted, so the listing is quietly one short
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

A key consisting only of slashes has an empty leaf name. `parse_list_bucket_result` drops it with a
bare `continue` **before** the `filtered_count` arm — so the entry is both **invisible and uncounted**,
and the listing is quietly one object short with nothing indicating it.

That matters beyond the missing row: CPE-1704 established a counting contract where the delete path
reads `entries.len() + filtered_count` as the true total. An entry that is dropped without being
counted breaks that arithmetic silently.

## Why it was left

Found by the CPE-1721/1722 worker while fixing the path grammar. It deliberately did **not** change it
inside that PR — altering a counting contract in the middle of a path-grammar change is the wrong risk,
and the two concerns are independent.

Instead it **pinned the current behaviour at `0`**, so the guard reds if anyone touches it. That is the
right shape: the bug is now impossible to change accidentally, and this ticket is what changes it
deliberately.

## What to do

- Decide what the correct behaviour is before writing code. A slashes-only key is legal in S3 but has
  no displayable leaf name — so the options are roughly: count it as filtered (honest, invisible, the
  arithmetic works), or surface it with some synthesised display name (visible, but invents a name the
  bucket does not contain). Weigh them and **say which and why**; do not let the implementation decide.
- Whichever way, `entries.len() + filtered_count` must stay a true total, per CPE-1704.
- **Red-proof it**: a `ListBucketResult` containing a slashes-only key must produce a count that
  reconciles, and the existing pin (currently asserting `0`) must be updated deliberately rather than
  deleted — if the pin disappears in the diff, the guard went with it.
- Check the sibling arms for the same shape: any other `continue` in that parser that skips an entry
  without reaching the counting arm has the identical bug.

## Notes

Filed by the Foreman from PR #955, 2026-08-20. Worth noting the worker found this while doing something
else, declined to fix it in-flight, and left a failing-if-touched pin rather than a comment — which is
why it is a ticket instead of a discovery someone makes in six months.

Related: **CPE-1704** (the counting contract), **CPE-1722** (the path grammar that surfaced it).
