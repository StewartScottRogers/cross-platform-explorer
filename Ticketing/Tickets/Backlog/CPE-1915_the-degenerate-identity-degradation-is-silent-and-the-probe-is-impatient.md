---
id: CPE-1915
title: on a share that cannot report file identity, backup's escape detection silently weakens — and the race probe is too impatient to prove the fix on Windows
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

Two loose ends from CPE-1896's second security audit, which returned `SEC PASS`. Neither blocks that
work; both were explicitly recommended as follow-ups by the auditor that proved the original bypass.

## 1. The degradation is silent, and silence is the objectionable part

CPE-1896's landing check compares **file identity** — did the bytes go into the object we wrote? On a
volume that returns a degenerate identity (zero volume or zero index, which several network redirectors
do), there is nothing to compare, so it falls back to the containment answer and the swap-back window
reopens. Confirmed by direct exercise of all three shapes: no identity model, zero volume, zero index —
all three admit.

**Taking the fallback is the right call and this ticket does not propose changing it.** The auditor's
reasoning, which I agree with: refusing here does not mean declining to act, it means reporting **every
file of every backup to an affected share as failed** — on the destination type backups are *for*. A
backup product that reds every entry gets switched off, which is a worse security outcome than the
residual. It is a graceful degradation, not a fail-open: behaviour on such a volume is exactly what
shipped before the identity check, which still catches the single-phase escape (73/1200 of the measured
harm). Nothing regresses; some volumes just do not get the new half. And the asymmetry is right — an
open that **fails** refuses, an identity that **cannot discriminate** degrades. "I could not look" and
"looking tells me nothing" deserve different answers.

**What is missing is that the user is never told.** They get the weaker guarantee with no indication.
The docs page mentions it, which is good, but a docs sentence is not a signal at the moment it matters.

Note on reachability, because it is worse than "not attacker-chosen": the attacker does not pick the
destination, but they need no extra capability either — presence of a degenerate-identity volume is
decided by **the user's hardware**. Someone backing up to a NAS or SMB share whose redirector zeroes the
file index is back to the pre-fix exposure with only the base precondition. This project's own standing
test target is a QNAP on the LAN, so it is not hypothetical hardware.

## 2. The race probe cannot red-proof the identity leg, and it is not the platform's fault

`cpe_1896_a_parent_swapped_under_the_copy_is_never_reported_as_a_success` retries its rename-back 2,000
times at 10 µs — a **20 ms** budget — and never lands the swap-back on Windows. The auditor's racer waits
for the outside file to pass 4 KiB, then retries against a **400 ms** deadline, and lands it 214 times in
400 trials on the same machine.

So the probe is impatient, not blocked. With a wider deadline it bites at roughly **2%** of trials on
Windows, which would make it red-proof the identity comparison on all three platforms instead of two —
and would let its own doc drop the caveat that it must not be cited as proving that.

## Acceptance criteria

- [ ] Surface the degradation once per run, at the job level: something like *"this destination cannot
      confirm file identity; escaped-write detection is weaker here"*. Once per run, not per file — a
      per-file notice on a large backup is noise that trains people to ignore it.
- [ ] Do **not** turn the degradation into a refusal. Re-read the reasoning above before changing that;
      it was argued carefully and the alternative is worse.
- [ ] Widen the probe's swap-back retry deadline to a few hundred milliseconds so it lands on Windows,
      then confirm it red-proofs the identity comparison there — neutralise the comparison and watch it
      go red. Update its doc comment to drop the "does not red-proof the identity comparison" caveat once
      that is true.
- [ ] Keep the probe's safety-property assertion exactly as it is. It asserts *"if a write escaped, it
      was never reported as a success"* and nothing about a rate; the escape rate varies by an order of
      magnitude across volumes on one machine (1/600 vs 4-5/600, measured). Do not "harden" it by
      asserting a count.
- [ ] Record whether the QNAP target actually exhibits degenerate identity. That turns this ticket's
      central assumption from plausible into measured, and the hardware is on the LAN. Pairs naturally
      with CPE-1518 and CPE-1895, which both need the same NAS.

## Notes

Filed 2026-08-26 from CPE-1896's round-2 security audit, which closed its own blocking finding by A/B —
same racer, same 400 trials, identity neutralised then live: 9 `ok: true` became 0, with the swap-back
completing 214 times in both.

The auditor also names the permanent fix, and it is better than identity: `GetFinalPathNameByHandleW` /
`F_GETPATH` on the open handle answers the containment question **directly** rather than by proxy, works
regardless of whether the volume supplies a usable index, and **would close CPE-1912 for free** — because
the handle's real path can be compared against the *plan* path, not merely against the root. Whoever
picks up CPE-1896's remaining atomic half should look at that first.

Related: **CPE-1896** (still open — only the mitigation half shipped), **CPE-1912** (a junction inside
the root, which the handle-path approach would also close), **CPE-1913** (the four other sites with no
landing check), **CPE-1518** / **CPE-1895** (the QNAP target).
