---
id: CPE-1747
title: The fast deadline tests miss a value-dependent transport break, and one comment names the wrong ceiling
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found by the PR #907 (CPE-1713) reviewer, 2026-08-14, and **reproduced**. Filed rather than folded in
because CPE-1713 explicitly scoped itself to the two disconnection modes its own ticket named, and both of
those are caught. This is the residual.

CPE-1713 replaced two 60-second live-fire tests with a triple per crate: **mechanism** (a deadline field
bounds the call, driven with a short *injected* duration), **value** (the shipped constant is finite and
sane), and **wiring** (`connect()` assigns the shipped constant to the field the call site reads).

The mechanism test differs from production **only in the value of the deadline**. So any breakage that is a
*function* of that value is invisible to all three tests.

### Measured reproduction (reviewer, on PR #907's head)

`crates/s3/src/provider.rs:1259`:

```rust
if let Some(deadline) = request_deadline {
    if deadline <= Duration::from_secs(1) {   // added
        req = req.timeout(deadline);
    }
}
```

- Full `cpe-s3` suite: **189 passed, 0 failed, 1.13s.** Wiring, value, and mechanism all green.
- The deleted live-fire, restored verbatim and run against that same mutation: **FAILED — "did not return
  within 150s"**. The shipped `list` path is *completely unbounded* against a dribbling server while the
  whole suite is green.
- The same live-fire on unmutated PR-head code: passed in 60.01s.

### The related loss in the same family

The deleted test asserted a **lower** bound (`elapsed >= TIMEOUT_LIST_REQUEST`), proving nothing *tighter*
than the shipped deadline ended the call. Nothing in the surviving triple asserts a lower bound, so a
mutation that silently **shrinks** the effective deadline — e.g. `req.timeout(deadline.min(Duration::from_secs(5)))`
— also passes everything. That directly contradicts the value test's own floor rationale, which is that a
deadline under 5s cuts off legitimate slow gateways.

## Why this was not a merge gate

The escaping mutation is **adversarial, not a plausible accident**, and CPE-1713's own text proposed the
field assertion as the closure. Its fallback clause ("if the cheap replacement cannot be proven to catch a
disconnected constant, keep the 6 minutes") is **not** triggered — a disconnected constant *is* caught. The
gap is narrower than that clause: a transport break whose behaviour depends on the deadline's magnitude.

## The cheap closure the reviewer proposed

1. A **lower-bound assertion** in the mechanism test (the call must not return materially *before* the
   injected deadline), restoring the property the live-fire's `elapsed >= ...` carried.
2. A **second mechanism run at a larger injected duration**, so a value-conditional branch cannot be true
   for the one duration the suite happens to use.

Both stay in the millisecond-to-low-seconds range. Do **not** reintroduce a 60-second wait — the whole point
of CPE-1713 was the ~6 minutes of CI wall clock per run (2 × 60s × 3 OSes).

## Second, unrelated defect — a comment names the wrong ceiling

`crates/s3/src/provider.rs:3774` and `:3789`, and `crates/webdav/src/lib.rs:962` and `:977`, say the
mechanism test reds because "it runs past this test's own 10 s ceiling". Measured: it never returns at all,
and the red comes from the **30s `call_with_deadline` hang guard**, with a different message. PR #907's body
states this correctly; the committed comment — which is CPE-1713's own "write down which test covers which
property" artifact — does not. One-line fix per site.

## Acceptance criteria

- [ ] The value-dependent mutation above (guard the `.timeout()` behind `deadline <= 1s`) turns a
      **distinct** test red, in both `cpe-s3` and `cpe-webdav`.
- [ ] A deadline-shrinking mutation (`deadline.min(5s)`) turns a **distinct** test red in both crates.
- [ ] Neither addition costs more than ~2s of suite wall clock per crate. Measure and record before/after.
- [ ] The three existing properties (mechanism / value / wiring) each still red distinctly — re-run
      CPE-1713's four mutations and confirm nothing collapsed into a shared failure.
- [ ] The four comment sites name the ceiling that actually fires (the 30s hang guard) and the message it
      emits.
- [ ] Any timing assertion carries generous slack — CI runs a 3-OS matrix under load, and GUI-smoke
      contention already reds unrelated PRs (CPE-1728).

## Notes

Related: CPE-1713 (this ticket's parent, PR #907), CPE-1706 (which shipped the live-fires and, in its
round 1, the disconnected constant that motivated all of this).
