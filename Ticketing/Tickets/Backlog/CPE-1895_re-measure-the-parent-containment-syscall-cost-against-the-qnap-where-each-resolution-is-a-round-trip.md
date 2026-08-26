---
id: CPE-1895
title: re-measure the parent-containment syscall cost against the QNAP, where each resolution is a round trip
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

CPE-1889 added parent-directory containment to the backup write leg. Its cost, stated honestly by
its own author and confirmed by its reviewer, is **+1 `metadata` and +1 `canonicalize` per file**,
with the destination root canonicalised once per run. On a 100k-file backup that is roughly **200k
extra path resolutions**.

Local wall-clock A/B over 2,000 files on NTFS came out **+11.3, −67.0, −21.2, +29.2 µs/file** across
four runs — both signs, swamped by the copy's own variance. The author deliberately refused to quote
the first (favourable) run and reported the syscall count as the durable number instead. That was the
right call, and it leaves the real question open rather than answered.

**Local NTFS is the wrong instrument.** A path resolution against a local disk is cheap enough to
vanish into noise. Against a network destination each one is a round trip, and 200k of them is not
noise. The repo has a real QNAP TS-133 on the LAN precisely for this class of question.

This ticket exists because the concern was flagged in a code comment and nowhere else. CPE-1889's own
reviewer called that out as a process miss against this repo's convention: a deferred concern gets a
ticket, not just a doc comment, or it evaporates.

## Acceptance criteria

- [ ] Measure a backup to the QNAP with and without parent containment, at a file count large enough
      that the per-file cost is not swamped — state the count and why it is sufficient.
- [ ] Record the host and tool versions that produced the measurement, from a printed probe, not
      "whatever was on PATH". Both numbers come from the same session and the same hardware.
- [ ] Report both signs if the result is noisy. Do not quote a single favourable run — that is the
      exact failure this ticket's parent avoided.
- [ ] Decide, on the measured number, whether the per-file `canonicalize` needs amortising (a cache
      of already-resolved parent directories within a run is the obvious lever, since a backup walks
      many files per directory) and file that as its own ticket if so.
- [ ] If the cost turns out to be material, weigh it explicitly against PURPOSE.md's fast/small/
      predictable tiebreaker and record the argument — noting that correctness of "does not write
      outside the folder the user chose" outranks speed, so the outcome is an optimisation ticket,
      never a proposal to remove the guard.

## Notes

Filed 2026-08-26 from CPE-1889's independent review. Related: **CPE-1889** (the containment itself),
**CPE-1518** (the standing QNAP E2E verification ticket — likely worth running these together, since
both need the NAS and the setup cost is the same).

See [[qnap-nas-test-target]]. Note the measurement should use the real NAS rather than a simulated
latency, because the interesting variable is how the filesystem driver batches or caches resolutions,
which an artificial delay would not reproduce.
