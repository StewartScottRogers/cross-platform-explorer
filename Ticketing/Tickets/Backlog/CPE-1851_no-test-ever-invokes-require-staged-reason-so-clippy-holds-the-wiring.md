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

## Work Log

**2026-08-22** — Implemented. One new test, `fsutil::tests::cpe_1851_require_staged_reason_panic_names_the_step_it_was_given`
(`crates/server/src/fsutil.rs`), 257 added lines, no production code changed.

### Why it runs in a child process, not `catch_unwind` in-process

The `Fail` arm is only reachable when `staging_is_strict()` is true, and that reads the process-global
`CPE_STAGING_STRICT`. `cargo test` runs the `cpe-server` lib tests on many threads in ONE process, and
`archive.rs`, `copilot.rs` and `dispatch.rs` all call `require_staged` from their own tests — so setting
the variable in-process would flip *their* legitimate skips into panics, non-deterministically, on any
machine or runner that lacks symlink privilege or ACL support. A mutex would not help: those tests do not
take it. The `shell_menu.rs` `HOME_ENV_LOCK` pattern relies on being the only toucher of `$HOME`, which is
exactly the property that does not hold here.

So no lock and no env mutation in this process at all. The test re-executes its own test binary
(`std::env::current_exe()`) with `--exact <this test> --nocapture --test-threads=1`, `CPE_STAGING_STRICT=1`,
`CPE_STAGING_SABOTAGE` explicitly removed, and a private marker variable that makes the child take the
work branch instead of spawning again. The child runs exactly one test, so there are no neighbours to
corrupt. The parent asserts the child exited 0 **and** printed a completion sentinel — so "the filter
matched zero tests and exited 0" (a rename, a module move) reds instead of certifying nothing.

### What the child asserts

1. **Preconditions** — `staging_is_strict()` true and `staging_is_sabotaged()` false, or every assertion
   below would be vacuous against a function that never entered the `Fail` arm.
2. **The reason reaches the panic** — the message contains `[CPE-1717]`, the mechanism, and the exact
   reason nonce passed in, on the SAME first line (the line CI's `grep -m1 -A6 'CPE-1717'` always prints).
3. **The fixture is live** — the same call is made twice with two DIFFERENT reason nonces. Neither message
   may contain the other's nonce, the two messages must differ byte-for-byte, and neither may equal
   `staging_failure_message(mechanism)`. This is the anti-inert assertion: if `require_staged_reason`
   emitted a constant, two different reasons would produce identical text, and every `contains(REASON)`
   assertion above would pass against a hardcoded string. Phrased in the CPE-1823 round-5 style
   ("fixture is inert: ... this test certifies nothing").
4. **`#[track_caller]`** — a panic hook records the reported file+line for all three panics; each must be
   the call site's own line (captured with `line!()`), not the helper's body. Both helpers live in the
   same file, so only the LINE discriminates — verified below.
5. **The sibling `require_staged`** — same treatment (see below).
6. **The arms that must not panic** — `Staged` returns true, `LegitimateSkip` returns false.

### Red-proofs (all run live in this worktree, `crates/server`)

| # | Mutation | `cargo clippy --all-targets -D warnings` | `cargo test` |
|---|----------|------------------------------------------|--------------|
| 1 | round-2 shape, the one clippy misses: `panic!("{}", staging_failure_message_with_reason(mechanism, staged_fail_reason(Err("staging failed"))))` | **CLEAN** (0 warnings) | **RED** — `require_staged_reason panicked WITHOUT the reason it was handed` |
| 2 | round-1 shape: `Fail => panic!("{}", staging_failure_message(mechanism))` | red (`function staged_fail_reason is never used`) — but that is now the *second* holder | **RED** on the same assertion, independently of clippy |
| 3 | reason hardcoded to nonce A specifically (passes the single-nonce check, fails the liveness check) | clean | **RED** — `the second reason must reach its own message too` |
| 4 | `require_staged`'s `Fail` arm gutted to `panic!("[CPE-1717] \`{mechanism}\` could not stage")` | **CLEAN** (0 warnings — `staging_failure_message` is `pub`, so no dead-code lint) | **RED** — `require_staged must panic with exactly staging_failure_message(mechanism)` |
| 5 | `#[track_caller]` removed from `require_staged_reason` | clean | **RED** — reported line 3035 (helper body) vs expected 4197 (call site) |

Mutation 1 is the acceptance criterion: clippy stayed clean and the new test went red. Mutation 2 confirms
the test, not clippy, is now what catches the round-1 shape — under mutation 2 `cargo test` still compiles
(dead-code is a warning, not an error, outside `-D warnings`) and the test reds on the substance.

### `require_staged` — the sibling: **yes, it had the same hole, and it is now closed too**

`require_staged` (~36 call sites) had no test invoking it either, and its hole is arguably worse: the
round-1-style mutation on it is not even clippy-visible, because `staging_failure_message` is `pub` in a
library crate and so is exempt from the dead-code lint that incidentally guarded `staged_fail_reason`.
Measured as mutation 4 above: replacing its panic body with an arbitrary string left clippy at 0 warnings.
The new test therefore exercises `require_staged`'s `Fail` arm in the same child run — asserting the panic
text equals `staging_failure_message(mechanism)` exactly, that it names the mechanism, that it does NOT
invent a "failing step:" clause it has no reason for, and that `#[track_caller]` holds for it as well.
Fixing only the function named in the ticket would have left the larger of the two gates uncovered.

### `#[track_caller]`

Still reports the caller's line, for both functions, asserted rather than eyeballed — and the assertion
discriminates: removing the attribute moves the reported line from the call site (4197) to
`require_staged_reason`'s own body (3035) and reds. Same holds for `require_staged`.

### Gates (real numbers, with deltas accounted)

- `crates/server` `cargo clippy --all-targets -- -D warnings`: **clean**.
- `crates/server` `cargo test`: lib **2314** passed / 0 failed / 4 ignored. Baseline measured on this
  branch's merge-base *before* the change was **2313** / 0 / 4, so the delta is exactly **+1**, the one
  new test. (CPE-1815's Work Log recorded 2289; the +24 since is other merged tickets, not this one — the
  2313 figure was measured here, not inherited.) Integration binaries unchanged:
  `archive_panic_safety` 21, `binary_data_preview_panic_safety` 22, `checkpoint_roundtrip` 2,
  `finder_tags_os_interop` 1, `native_meta_os_interop` 1, `parser_panic_safety` 45, `sample_fixtures` 16,
  `thumb_svg_panic_safety` 32, `ticket_mcp` 0, doc-tests 0.
- `src-tauri` clippy `--all-targets -- -D warnings`: clean in **both** feature modes (default and
  `--features sidecar-platform`). `cargo test`: **214** / 0 (default) and **269** / 0
  (`sidecar-platform`) — unchanged from CPE-1815's numbers, as expected: no `src-tauri` file was touched.

### Not verified

- Behaviour on the Linux and macOS CI runners. The test has no `cfg`, no filesystem access and no
  platform-specific assertion (the message's only platform-varying text is `std::env::consts::OS`, which
  is deliberately not asserted on), but the child-process spawn itself has only been exercised on Windows
  locally. CI's 3-OS backend matrix covers it on this PR.
- Cost: the test spawns one extra test-binary process. Measured ~0.02 s locally; it is not a hot path, but
  it is a process spawn rather than a pure in-process call, which is the price of not mutating a
  process-global that other tests read.
- What the test still does NOT catch is written into its own doc comment, in the
  `half_applied_rename_guards_are_rejected` "# What it does NOT catch — measured, not guessed" style:
  a reason *misattributed* to a different per-call value (a constant substitute IS caught, since both
  messages would then be equal); which of the seven real trash reasons a given probe step picks (that is
  the producer end, pinned by `src-tauri`'s two guard tests); and the `LegitimateSkip` caller-side
  `(cause: {})` skip-notice plumbing, of which this only asserts the arm returns `false` without panicking.
