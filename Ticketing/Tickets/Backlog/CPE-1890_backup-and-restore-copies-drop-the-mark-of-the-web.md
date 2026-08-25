---
id: CPE-1890
title: backup and restore copies drop the Mark-of-the-Web, and an overwrite keeps the previous file's
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-25
closed:
---

## Problem

`std::fs::copy` on Windows is `CopyFileExW`, which carries NTFS alternate data streams.
`fsutil::copy_file_onto_no_follow` — now used by the restore writer (CPE-1857) and the backup writer
(CPE-1879) — is open → `set_len(0)` → byte stream, which carries none.

Measured by the Security Auditor on PR #1022:

```
A5 ADS staged on source: true
A5 fs::copy   -> Zone.Identifier present: true      (main)
A5 no_follow  -> Zone.Identifier present: false     (branch)

A6 stale Zone.Identifier survived no_follow overwrite: true
A6 stale Zone.Identifier survived fs::copy overwrite:  false
```

So a copy **drops the source's zone tag and keeps the previous occupant's**. Concretely: back up or
restore `Downloads`, then open a file from it — SmartScreen does not prompt and Office does not open it
in Protected View.

## Why this is not simply "a known accepted cost"

The ADS loss **is** documented and deliberately accepted inside `copy_file_onto_no_follow`
(CPE-1845 / 1846 / 1870 — the stream carry was implemented, lost a race to a round-2 auditor, and was
removed on measured grounds). But read the acceptance reasoning:

> *"the bytes are the user's own captured content **from a local store**, and the direction of the
> error is toward **keeping** an existing warning rather than dropping one on the overwrite path."*

**Both halves are false at the newer call sites.** For backup the source is the user's arbitrary tree,
downloads included — not an app-private blob store. And the direction of the error is toward
**dropping** the warning, which is exactly the case the original reasoning excluded.

## macOS — likely, and deliberately not asserted

`std::fs::copy` on macOS uses `fclonefileat` / `fcopyfile(COPYFILE_ALL)`, which carries extended
attributes including `com.apple.quarantine`. So Gatekeeper's equivalent tag is **probably** dropped
too. The auditor could not test it (no macOS available) and did not claim it. **Measure it before
fixing it** — the 3-OS CI matrix can pin this. Do not repeat the pattern of asserting an unmeasured
platform claim; that has already cost this repo two round-trips.

## What to do

1. **Carry the streams**, and read why the previous attempt was removed before re-adding it — the
   reasoning is in `fsutil.rs` around the CPE-1845/1846/1870 comments. Do not resurrect a version that
   was measured and rejected without addressing what it was rejected for.
2. **Fix the overwrite direction too.** Keeping the *previous* file's zone tag is arguably worse than
   dropping the new one — it attaches a stale trust judgement to different bytes. Both A5 and A6 must
   flip.
3. **Measure the cost.** The earlier removal was on measured grounds, so measure again and put the
   numbers in the work log. `fsutil.rs`'s own benchmark table is the precedent.
4. **Decide the scope deliberately**: restore, backup, archive extraction and transfer download all now
   share this write path. State which get the carry and why any do not.

## Acceptance criteria

- [ ] A backed-up / restored file keeps its `Zone.Identifier` — demonstrated, both A5 and A6.
- [ ] macOS `com.apple.quarantine` behaviour **measured** and stated, whichever way it falls.
- [ ] Cost measured and recorded against the earlier removal's numbers.
- [ ] `src/docs/safety-undo.md`'s note that backup copies do not carry the downloaded-from-the-internet
      mark is removed or corrected once it is no longer true.

## Work Log

- **2026-08-25 15:30 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`, from
  the Security Auditor's finding on PR #1022. Kept out of that PR deliberately: the loss is inherited
  from a shared helper whose removal was itself a measured decision, so re-adding it is its own piece of
  work. CPE-1879 shipped stating the gap plainly in both the code and the user docs instead.
