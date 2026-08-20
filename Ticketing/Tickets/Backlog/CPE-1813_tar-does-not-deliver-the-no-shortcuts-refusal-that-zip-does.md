---
id: CPE-1813
title: TAR does not deliver the no-link-support refusal that ZIP does, so the two formats still disagree
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

CPE-1759 made a filesystem that cannot hold symlinks — a FAT stick, say — produce a **counted refusal**
rather than a dead extraction. That refusal is delivered for **ZIP only.**

For TAR, `unpack_in` creates the link itself and its error **aborts** (`crates/server/src/archive.rs:3318`);
`materialise_entry_symlink` is never on that path. So the very divergence CPE-1759 existed to close —
*the two formats answering the same situation differently* — survives in this one case.

## Why it matters

The rule CPE-1759 established is **refusals skip, failures abort**, and "this filesystem has no shortcuts"
is squarely a refusal: trying the next entry can plausibly work, and the other 499 files are still worth
having. TAR gives the user a dead extraction instead.

It is also the case most likely to be met in real life by someone doing something ordinary — extracting a
source tarball onto a USB stick.

## What to do

- Decide whether to route TAR's link creation through the same classifier, or to leave `unpack_in` in charge
  and translate its error. **Say which and why** — `unpack_in` owning the write is deliberate and CPE-1759
  already retracted one bad argument in this area, so do not re-derive that reasoning from the shape of the
  code.
- Whichever way, `WINDOWS_NO_LINK_SUPPORT` and POSIX `EPERM`/`EACCES` handling must end up **stated once**
  rather than duplicated per format — a second copy is how the two formats diverged in the first place.
- **Then fix the in-app help.** `src/docs/explorer-archives.md` should describe what actually ships. This
  file has carried a factually wrong statement repeatedly, so re-read the code before writing the sentence.
- Red-proof both formats in one test, the way CPE-1759's `one_shot_and_streamed_zip_answer_a_link_at_an_entry_name_identically`
  does — a per-format test cannot catch a divergence.

## Notes

Filed by the Foreman from the round-3 re-review of PR #958, 2026-08-20. Found because the reviewer checked
a doc sentence against the code rather than accepting it.

Related: **CPE-1759**, **CPE-1773/1774/1775**.
