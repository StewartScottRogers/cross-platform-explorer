---
id: CPE-1779
title: A partially-written extraction leaves a real file on disk that Agent Watch never records
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-18
closed:
---

## Problem

Found by the PR #932 (CPE-1745) UAT, which went looking for the in-between case rather than testing only
the happy and error paths.

CPE-1745 moved the three archive single-entry extraction commands to record their `note_app_op` path
**after** the extraction, using the real returned value — replacing a hand-mirrored guess that had been
naming a path which did not exist. That fix is right, and success-only recording was the deliberate,
recorded decision: nothing written, nothing recorded.

The gap is that "nothing written" is not always true when the call returns `Err`. **An extraction can write
real bytes to disk and still fail.** Reproduced with a 200,000-byte incompressible payload, zipped, then
corrupted mid-stream:

```
cut_frac=40 -> Err(corrupt deflate stream)
             PARTIAL FILE on disk: ...\cpe-archive\7232-2679\big.bin, 81895 bytes (full is 200000)
             NOT recorded by note_app_op
cut_frac=50/60/70 -> Err(Invalid checksum)
             file FULLY written (200000/200000) but CRC failed
             NOT recorded
```

The checksum case is the sharper one: the file is complete on disk, byte for byte, and the only thing wrong
is that the CRC did not match. Agent Watch says nothing about it at all.

## Why it is Medium, not High

This is the **opposite** failure mode from the one CPE-1745 fixed, and the lesser of the two:

- Before CPE-1745: a confident claim about a file that never existed. Actively misleading.
- Now: silence about a file that does exist. Under-reporting.

Silence does not send anyone to the wrong place. But Agent Watch's whole purpose is showing what actually
happened on disk, and a leftover file in `%TEMP%` that the record does not mention is still a hole in that
promise — particularly since these leftovers are exactly what CPE-1693 is about, and they are accumulating.

## What to do

- Decide what a failed-but-wrote-something extraction should record, and record the reasoning at the call
  site as CPE-1745 did for the success-only choice. The plausible answers: record the path with a
  distinguishing marker (attempted / partial), record it plainly since a file is genuinely there, or have
  the server clean up its own partial output on failure so "nothing written" becomes true again.
- **That last option is worth serious consideration** — it fixes the record and the litter in one move, and
  it makes the simple success-only rule correct rather than approximately correct. Check whether
  `temp_extract_target`'s caller can remove the partial on the error path without racing anything.
- Whatever is chosen, the test must assert against a **real** partially-written file — the UAT's corrupted
  deflate stream and CRC-mismatch fixtures are the reproduction, reuse them rather than mocking an `Err`.
- Check the other extraction sinks for the same shape, not just these three commands.

## Acceptance criteria

- [ ] A corrupt-deflate extraction and a CRC-mismatch extraction each behave per the recorded decision, and
      the decision is written at the call site.
- [ ] If the choice is "clean up the partial", assert the file is **gone** after the failure; if it is
      "record it", assert the recorded path equals the partial file's real path.
- [ ] Breaking the behaviour reds a **distinct** test naming the partial-write case.
- [ ] The success path is unchanged — CPE-1745's equality assertion still holds.
- [ ] Any test creating a scratch directory arms a `Drop` guard **before** its assertions (CPE-1693).

## Notes

Found by UAT on **PR #932 / CPE-1745**, 2026-08-18, during the batched sprint; judged a lesser harm than the
defect that PR fixed, and correctly not treated as a blocker. Related: CPE-1745, CPE-1195 (the
`<pid>-<seq>` extraction directory), CPE-1693 (the leftovers these add to).

## The reviewer converged on this independently, and prescribed the fix

The #932 reviewer reproduced the same case from a different angle — a 2 MB entry corrupted mid-stream,
returning `Err("Invalid checksum")` after `File::create` + `io::copy` had already written **2,000,259
bytes** — and reached the same conclusion. Two independent legs finding the same thing is the strongest
signal this crew produces.

It also identified the cleanest fix, which is the "clean up the partial" option above:

> `crates/server/src/archive.rs::extract_archive_entry` and its siblings `extract_tar_entry` /
> `extract_7z_entry` / `extract_rar_entry` (~lines 793, 808, 832, 907) can leave a full-size orphaned file
> when the copy or decode fails after `fs::File::create`. On the `Err` branch of `std::io::copy` /
> `fs::write` in those functions, `let _ = fs::remove_file(&out);` before propagating the error — so a
> failed extraction leaves nothing behind rather than relying on CPE-1693's general sweep.

Prefer this over recording the partial. It makes "nothing written, nothing recorded" **true** rather than
approximately true, removes the orphan instead of documenting it, and needs no change to CPE-1745's rule.

Two scoping facts the review established, both worth keeping:

- **This is pre-existing, not a regression.** The same `File::create`-then-copy shape predates CPE-1745, and
  the old code's before-the-call record was *also* wrong for this file (it guessed a flat path that never
  existed). Failure-path accuracy is unchanged, not worsened.
- **No consumer is harmed today.** `note_app_op`'s ledger has exactly one consumer, `resolve_actor`, which
  matches entries against filesystem-watcher events inside a session's watched **project folder**. Grepping
  `src/` for `app_ops` / `note_app_op` returns nothing — it is never surfaced to the frontend as an
  "attempts" list. `%TEMP%/cpe-archive/...` sits outside any watched project folder in the ordinary case.
  Flagged as unverified: nothing structurally *prevents* `%TEMP%` from falling inside a user-chosen
  `agent_watch_start` root. Worth checking when this is picked up.
