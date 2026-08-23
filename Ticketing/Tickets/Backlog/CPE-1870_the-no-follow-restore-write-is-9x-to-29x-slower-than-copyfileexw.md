---
id: CPE-1870
title: the no-follow restore write is 9x-29x slower, and the benchmark that cleared it measured the wrong shape
type: task
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-23
closed:
---

## Problem

CPE-1846 closed a demonstrated arbitrary-file-write by replacing `fs::copy` (`CopyFileExW`) with a
no-follow open plus a user-mode handle write. That was the right trade against 63 escapes per 10 rounds on
the shipping revert path. It also costs, and nobody measured it until the audit:

| workload | `fs::copy` | `copy_file_onto_no_follow` | factor |
|---|---|---|---|
| 3,000 x 19-byte files | 1,485 ms | 13,678 ms | **9.2x** |
| 500 x 256 KiB (128 MB) | 329 ms | 9,628 ms | **29x** (389 MB/s → 13 MB/s) |

A 2 GB checkpoint revert goes from about 5 seconds to about **2.5 minutes**. That is a user-visible
operation and it lands against PURPOSE.md's fast/small/predictable tiebreaker.

## The guards are not the cost

The audit bisected six variants rather than assuming:

```
as shipped                                        9640 / 9653 ms
BASELINE fs::copy                                  332 /  338 ms
set_len(0) removed                                9875 / 9660 ms
set_permissions removed                           9599 / 9708 ms
carry_file_times removed                          9553 / 9724 ms
both removed                                      9636 / 9598 ms
ALL guards gone: plain File::create + stream      9493 / 9514 ms   <- same as shipped
```

A bare `File::create` + `write_all` with **zero** guards costs the same. The entire regression is inherent
to leaving `CopyFileExW` for a user-mode handle write — consistent with Defender scanning each file on
close. So removing safety would buy nothing; the fix has to keep the handle-pinning property.

## Why the existing benchmark cleared it

`COPY_CHUNK`'s doc benchmarks **one 200 MB file** at 2,101 MB/s through a handle. That amortises a single
open/close/scan over 200 MB. **A restore is many files**, so it is all per-file fixed cost — 0.5 ms → 4.6 ms
per tiny file, 0.66 ms → 19.3 ms per 256 KiB file.

CPE-1846 reused that benchmark's conclusion for a shape it does not cover. That is the durable lesson here,
and it is worth carrying beyond this ticket: a throughput number from one large file says nothing about a
many-small-files workload.

## The shape of a fix

The property that must survive is **handle pinning** — the audit proved it by planting a link strictly
after the open, mid-copy of a 64 MiB blob, and getting `Ok(67108864)` with the victim untouched.

Candidate: `CopyFileExW` into a `create_new`-claimed sibling name in the same directory, then
`ReplaceFileW` or a rename onto the target. That keeps the kernel copy path and never writes through a
name an attacker can swap. It needs its own audit — a rename introduces its own window — but it is the
obvious place to start.

## Acceptance criteria

- [ ] Measure before changing anything, on both shapes: many small files and a few large ones. The 200 MB
      single-file benchmark is not evidence for either.
- [ ] Any fix keeps handle pinning. Re-run the audit's post-open plant (link planted after the open,
      mid-copy of a large blob) and report the result.
- [ ] Re-run the triggered race against both sinks and report live plants, entries written, and writes
      through. **Publish the denominator** — "N plants, 0 escapes" without the count of entries actually
      written is what produced two false negatives on CPE-1823.
- [ ] Say what happens to the ADS regression CPE-1846 accepted. A `CopyFileExW`-into-a-sibling route may
      carry streams again, which would close that too — check rather than assume.
- [ ] Red-proof every test with the minimal realistic change, observe red, revert, record the line.

## Notes

Found by the independent Security Auditor during CPE-1846, which it recommended merging: *"shipping a 29x
regression on a user-visible operation without it appearing in the record is the one gap in an otherwise
fully-measured ticket."* The numbers are now in CPE-1846's Work Log; this ticket owns the fix.

Read CPE-1846's Work Log first — it carries the guard layering, the ADS measurements and the reason
`create_new` cannot be used at this site (it refuses an existing name, and overwrite is the whole point).

Related: CPE-1846 (the security fix), CPE-1823 (the residual it closed).
