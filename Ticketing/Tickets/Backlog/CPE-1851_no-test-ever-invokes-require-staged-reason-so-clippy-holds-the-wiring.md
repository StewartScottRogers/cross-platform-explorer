---
id: CPE-1851
title: no test ever invokes require_staged_reason, so dead-code analysis is holding the wiring
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-21
closed:
---

## Problem

`require_staged_reason` (`crates/server/src/fsutil.rs:2968-2977`) is the function that turns a failed
staging probe into a panic naming which step failed. **No test in the tree ever invokes it.** The only
coverage is on `staging_failure_message_with_reason` in isolation.

CPE-1815's round-1 review found that dropping the reason in its `Fail` arm was caught **only** by
`cargo clippy --all-targets -D warnings`, as *"function `staged_fail_reason` is never used"*. Round 2
closed the **producer** end well — the reason array and its call-site wiring are now pinned by two tests
— but did not close the **consumer** end.

Round 2's review then found a variant **clippy does not catch either**:

```rust
panic!("{}", staging_failure_message_with_reason(mechanism, staged_fail_reason(Err("staging failed"))))
```

Every function stays "used". `staged` stays used via `staged.is_ok()`. The seven-way distinction is
erased at the exact point it is consumed. Measured: **2289 passed, 0 failed, clippy clean**.

## Why it matters

The whole point of CPE-1815 was that a CI failure should name which of seven steps broke. That value is
produced correctly and can still be discarded on delivery, silently, with every gate green.

It is also the eighth instance in this family of a guard held by something other than a test. The
recurring signature across CPE-1780, CPE-1806, CPE-1814, CPE-1815 and CPE-1823 is identical: **the test
never reaches the thing it claims to protect.** Here it is worse than usual — nothing reaches it at all,
and the incidental holder (dead-code analysis) evaporates the moment `staged_fail_reason` gains a second
caller.

## Acceptance criteria

- [ ] A test invokes `require_staged_reason` directly and asserts the panic message carries the specific
      reason it was given — not merely that it panicked. A `catch_unwind` under `CPE_STAGING_STRICT=1` is
      the shape the reviewer suggested.
- [ ] Red-proof with the **exact mutation above** — the one clippy misses. If the new test does not red
      under it, the gap is not closed. Also re-run the round-1 mutation (dropping the reason entirely) and
      confirm the test, not clippy, is what catches it now.
- [ ] Assert the test's own fixture is live: that the reason it passes in actually reaches the message,
      so the test cannot pass against a panic that never consulted it. Six inert tests were caught on
      CPE-1823 and in every one the fixture never reached the harm.
- [ ] Check whether `require_staged` (the older, reason-less sibling, ~36 call sites) has the same hole,
      and say so either way rather than fixing only the one named here.
- [ ] Confirm `#[track_caller]` still reports the caller's line after any change — CPE-1815's review
      verified this holds today and it must not regress.

## Notes

Filed from CPE-1815's round-2 review, which explicitly declined to block on it: the ticket's stated scope
was `trash_roundtrip_available`, the natural regression shape is still clippy-caught, and the panic text
itself is asserted elsewhere. Its own summary is the fairest statement of the position — *"clippy is
still doing the work, and there is at least one shape clippy misses too."*

Related: CPE-1815 (the reasons this delivers), CPE-1814 and CPE-1806 (the same "a skip is not a pass"
family), CPE-1848 (the other guard-held-by-accident finding from this batch).
