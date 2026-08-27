---
id: CPE-1920
title: `landed_inside`'s identity probe costs ~850 µs/file — almost all of it Defender re-scanning the just-written file — and CPE-1896 made it redundant on the happy path
type: task
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

CPE-1896's worker bisected the backup engine's per-file cost while measuring its own change, and
found that **the dominant cost is not the new containment walk — it is a pre-existing identity
probe** inherited from PR #1037's `landed_inside` check.

Measured, 2,000 files, guarded engine vs the pre-fix shape:

| component | µs/file (repeated runs) |
|---|---|
| whole engine as it ships | **+920.5 / +1093.2 / +945.7 / +960.8** |
| the landing check, minus its identity probe | +71.2 / +101.9 / +92.5 |
| **the new per-component walk alone** | +100.9 / **−51.1** / +19.6 — both signs, below the copy's own noise floor |

So ~**850 µs/file** is `landed_inside`'s identity probe. And it is **not syscall time**: the giveaway
is that the same file is opened twice, costing ~80 µs when only attributes are read versus ~850 µs
when data is read — i.e. **Windows Defender real-time scanning the just-written file on its first
read-open**.

## Why it is now redundant on the happy path

CPE-1896 replaced check-then-open with a per-component, handle-relative walk that refuses a link at
every step, so the destination handle is contained **by construction**. `landed_inside` was kept
deliberately, and correctly — it is the after-the-fact backstop for the one residual the walk does
not cover (an intermediate directory *object* renamed out of the root mid-copy, which `openat2` is
immune to but the Windows walk is not). But its **identity probe** re-opens and re-reads the written
file to establish sameness, and that is the expensive half.

## Acceptance criteria

- [ ] Establish which part of `landed_inside` is still load-bearing after CPE-1896 and which is not.
      Keep the residual coverage; the goal is to stop paying ~850 µs/file for a re-read, not to
      delete the backstop.
- [ ] Take the cheap route to file identity where one exists. CPE-1915's `GetFinalPathNameByHandleW`
      approach removes **both** re-opens and is already open as a ticket — coordinate with it rather
      than solving the same thing twice. On Unix, `fstat` on the handle already in hand gives
      dev/ino without a re-open at all.
- [ ] **Re-measure after the change, on the same machine, with Defender in the same state**, and
      record the host/toolchain/Defender-engine version alongside the number — the CPE-1896 Work Log
      has the format to copy.
- [ ] Confirm the residual attack (intermediate directory renamed out of the root mid-copy) is still
      caught, with a test that goes red if the backstop is weakened. This is the whole reason
      `landed_inside` was kept; a performance change that quietly removes its coverage would be a
      security regression wearing a benchmark.
- [ ] Weigh the result against PURPOSE.md's fast/small/predictable tiebreaker and record it.

## Notes

Filed 2026-08-27 by the sprint Foreman on the explicit recommendation of CPE-1896's worker, which
found this while measuring its own change and flagged that **its change is net cost-negative** — the
expensive part is inherited, not added.

Measurement provenance from that run: Windows 11 Pro 10.0.26200, x86_64, local NTFS (`%TEMP%`),
rustc/cargo 1.98.0 `x86_64-pc-windows-msvc`, Defender RTP on, engine 4.18.26070.9, debug profile.

Related: **CPE-1896** (the containment walk, which made this redundant), **CPE-1915**
(`GetFinalPathNameByHandleW` / weakened escape detection on shares that cannot report file identity),
**CPE-1895** (the remote/QNAP re-measurement of the same cost).
