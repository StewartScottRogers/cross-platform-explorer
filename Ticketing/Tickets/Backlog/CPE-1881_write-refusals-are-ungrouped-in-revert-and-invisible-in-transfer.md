---
id: CPE-1881
title: hard-link write refusals are ungrouped in revert (8 MiB of prose) and invisible in transfer (stderr only)
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-23
closed:
---

## Problem

CPE-1857 added a refusal when a write would land on a multiply-linked destination. The refusal is
correct. **How it is reported is not**, in two places, found and measured by the independent Security
Auditor on PR #1016.

### 1. Revert: one full 420-byte sentence per refused file

Each refusal carries the whole explanation — what a hard link is, why no path check can see it, and
how to break the link. Measured on a 200-file fixture:

```
CMD revert[200 hard-linked files]: applied=0 skipped=200 elapsed=1.26s
      total skipped-message bytes = 84180 (420 bytes/entry avg)
      extrapolated for 20,000 entries = 8220 KiB
```

**~8.2 MiB of identical prose across the IPC for a 20,000-entry revert.** This is not hypothetical:
a tree under `rsync --link-dest` or Time Machine-style dedup is hard-linked wholesale by design, and
legitimate in-root dedup refuses too (verified: two names both *inside* the root, both refused, as
designed).

**CPE-1847 already solved this exact shape** — it grouped hold-backs into one `HeldBackSummary` after
measuring 185 KiB as the problem. Write refusals bypass that grouping entirely, and are 44× worse.

### 2. Transfer: the user is told nothing at all

`download_tree`'s hard-link arm is an `eprintln!` and nothing else. The user sees the delivered count
silently one lower — no `undelivered` entry, no reason, no count of skips. Verified in the auditor's
run: the line went to stderr and `n == 0` was the only signal.

The PR argues this deliberately, and the reasoning is sound as far as it goes: an `undelivered` entry
would fail the *whole* transfer, which is worse. But that is a false choice between "fail everything"
and "say nothing" — **the third option was not considered.**

## What to do

1. **Revert:** fold the repeated sentence into the existing summary-plus-count shape CPE-1847 built.
   One explanation, once, plus the count and the paths. Reuse `HeldBackSummary`'s pattern rather than
   inventing a parallel one.
2. **Transfer:** carry per-entry skips out of `download_tree` as a **counted list**, the way
   `ArchiveReport.skipped` already does, so the user gets "N entries skipped, here is why" without
   failing the transfer.
3. While in `download_tree`, note the adjacent **symlink** arm has the same stderr-only shape. Fix
   both or state why not.

## Out of scope — do not fix here

- The one-shot registered `extract_archive` discards its `ArchiveReport` and returns `Ok(dest)`. That
  is pre-existing for *every* guard in that command and the GUI does not use it (the frontend uses
  `start_archive_extract`, which reports correctly). Recorded so it is not mistaken for coverage.

## Acceptance criteria

- [ ] A 200-file all-refused revert produces one explanation, not 200 — measured, with the before and
      after byte counts.
- [ ] A transfer that skips entries reports the count and reason to the user, without failing the
      transfer.
- [ ] The existing per-entry path information is not lost in the grouping.

## Work Log

- **2026-08-23 17:00 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from the Security Auditor's findings F3 and F4 on PR #1016. Both were measured rather than asserted.
  Neither blocks that PR: the refusal itself is correct and the alternative is the write going through.
